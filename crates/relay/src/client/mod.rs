use ::grpc::gen::mev_relay_client::MevRelayClient;
use builder_primitives::{
    bid::{BidTrace, SignedBid},
    relays::{RelayInfo, RelayName, SubmissionKind},
    validator::ValidatorRegistrationResponse,
};
use tokio::time::sleep;

use core::fmt::Debug;
use flate2::write::GzEncoder;
use flate2::Compression;
use reqwest::Client;
use reqwest::StatusCode;

use ssz::Encode;
use std::{io::Write, sync::Arc};

use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};

pub mod simulate;

static VALIDATOR_REGISTRATIONS_PATH: &str = "relay/v1/builder/validators";
static SUBMIT_BID_PATH: &str = "relay/v1/builder/blocks";
static BID_TRACES_PATH: &str = "relay/v1/data/bidtraces/builder_blocks_received";
static INTERNAL_BEST_BID_PATH: &str = "best_bid";

#[derive(Debug, Error)]
pub enum RelayClientErr {
    #[error("Cannot get the best bid from relay")]
    TopBid(#[from] reqwest::Error),
    #[error("Could not find a validator for slot {0}, might be a slot not covered by mev boost.")]
    ValidatorNotFound(u64),
    #[error("Relays returned empty validator registrations")]
    EmptyValidatorRegistrations,
    #[error("Error on submit: {0}")]
    Submit(String),
    #[error("Error on broadcast")]
    Broadcast,
    #[error("GZIP error")]
    Gzip(#[from] std::io::Error),
}

macro_rules! block_on {
    ($expr:expr) => {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on($expr))
    };
}

type GrpcClient = MevRelayClient<tonic::transport::Channel>;

/// RelayClient interacts with mev boost relays
#[derive(Clone, Debug)]
pub struct RelayClient {
    client: Client,
    info: RelayInfo,
    grpc_client: Option<GrpcClient>,
}

impl RelayClient {
    const MAX_RETRIES: u16 = 1000;
    const RETRY_DELAY: Duration = Duration::from_secs(2);

    pub fn new(relay: RelayInfo) -> Self {
        let grpc_client = if let SubmissionKind::Grpc(grpc_url) = &relay.submission() {
            Self::try_connect_grpc(Some(grpc_url))
        } else {
            None
        };

        Self {
            client: Client::new(),
            info: relay,
            grpc_client,
        }
    }

    fn try_connect_grpc(grpc_url: Option<&str>) -> Option<GrpcClient> {
        if let Some(url) = grpc_url {
            for _ in 0..Self::MAX_RETRIES {
                match block_on!(GrpcClient::connect(url.to_string())) {
                    Ok(client) => {
                        info!(?grpc_url, "Connected to gRPC");
                        return Some(client);
                    }
                    Err(e) => {
                        warn!("Failed to connect to gRPC: {}. Retrying...", e);
                        block_on!(sleep(Self::RETRY_DELAY));
                    }
                }
            }
            info!(
                "Failed to connect to gRPC after {} attempts",
                Self::MAX_RETRIES
            );
            None
        } else {
            None
        }
    }
}
#[async_trait::async_trait]
impl Relay for RelayClient {
    /// Get relay info field
    fn info(&self) -> RelayInfo {
        self.info.clone()
    }

    /// Get list of validators from flashbots relay, this is required to construct a bid
    /// fails fast if response is slow
    async fn get_validators(&self) -> Vec<ValidatorRegistrationResponse> {
        let url = format!("{}{}", self.info.url(), VALIDATOR_REGISTRATIONS_PATH);
        let name = self.info.name();
        let request = self.client.get(url.clone()).timeout(Duration::from_secs(2));

        match request.send().await {
            Ok(resp) => match resp.json::<Vec<ValidatorRegistrationResponse>>().await {
                Ok(data) => {
                    debug!(target: "relay::client", relay=name.to_string(), "Loaded {} validators", data.len());
                    data
                }
                Err(err) => {
                    error!(target: "relay::client", relay=name.to_string(), ?url, ?err, "Failed to get validators");
                    vec![]
                }
            },
            Err(err) => {
                error!(target: "relay::client", relay=name.to_string(), ?url, ?err, "Failed to get validators");
                vec![]
            }
        }
    }
    /// Submit bid to the given relay
    async fn submit_bid(&self, bid: Arc<SignedBid>) -> Result<SubmitBidResp, RelayClientErr> {
        let url = format!("{}{}", self.info.url(), SUBMIT_BID_PATH);
        let submission = self.info.submission();
        let request = self
            .client
            .post(url.clone())
            .timeout(Duration::from_secs(2));
        let grpc_client = self.grpc_client.clone();
        match submission {
            SubmissionKind::Ssz => {
                let resp = request
                    .header("Content-Type", "application/octet-stream")
                    .body(bid.as_ssz_bytes())
                    .send()
                    .await
                    .map_err(RelayClientErr::from)?;

                Ok(SubmitBidResp {
                    status: resp.status(),
                    message: resp.text().await?,
                })
            }
            SubmissionKind::Json => {
                let resp = request
                    .json(&bid)
                    .send()
                    .await
                    .map_err(RelayClientErr::from)?;
                Ok(SubmitBidResp {
                    status: resp.status(),
                    message: resp.text().await?,
                })
            }
            SubmissionKind::Sszgzip => {
                // Gzip encode the data.
                let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(bid.as_ssz_bytes().as_slice())?;
                let compressed_data = encoder.finish()?;

                let resp = request
                    .header("Content-Encoding", "gzip")
                    .header("Content-Type", "application/octet-stream")
                    .body(compressed_data)
                    .send()
                    .await
                    .map_err(RelayClientErr::from)?;
                Ok(SubmitBidResp {
                    status: resp.status(),
                    message: resp.text().await?,
                })
            }
            SubmissionKind::Grpc(_) => {
                let bid = bid.as_ref().clone();
                let request = tonic::Request::new(bid.into());
                let mut client = grpc_client.expect("Grpc client not initialized"); // here we can expect it as if we got SubmissionKind::Grpc, we should have initialized the client
                let resp = client.submit_block(request).await.map_err(|e| {
                    error!(target: "relay::client::grpc", ?e, "Failed to submit bid");
                    RelayClientErr::Submit(e.to_string())
                })?;
                let resp = resp.into_inner();
                Ok(SubmitBidResp {
                    status: if resp.code == 200 {
                        StatusCode::OK
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    },
                    message: resp.message,
                })
            }
        }
    }

    /// Get bid traces from given relay and block number
    async fn get_bid_traces(&self, block_number: u64) -> Option<(RelayName, Vec<BidTrace>)> {
        let url = format!("{}{}", self.info.url(), BID_TRACES_PATH);
        let response = self
            .client
            .get(url)
            .query(&[("block_number", block_number)])
            .send()
            .await;
        let name = self.info.name();

        match response {
            Ok(resp) => match resp.json::<Vec<BidTrace>>().await {
                Ok(traces) => {
                    debug!(target: "relay::client", relay=name.to_string(), "Loaded {} bid traces", traces.len());
                    Some((name.clone(), traces))
                }
                Err(_) => None,
            },
            Err(_) => None,
        }
    }

    /// Get best bid from internal relay
    async fn get_internal_best_bid(&self, block_number: u64, timestamp: u64) -> Option<SignedBid> {
        let url = format!("{}{}", self.info.url(), INTERNAL_BEST_BID_PATH);
        let name = self.info.name();
        let response = self
            .client
            .get(url)
            .query(&[("block_number", block_number), ("timestamp", timestamp)])
            .send()
            .await;

        match response {
            Ok(resp) => match resp.json::<Option<SignedBid>>().await {
                Ok(Some(bid)) => {
                    debug!(target: "relay::client", block=block_number, timestamp=timestamp, "Loaded internal best bid");
                    Some(bid)
                }
                Ok(None) => {
                    warn!(target: "relay::client", relay=name.to_string(), "No internal best bid found");
                    None
                }
                Err(err) => {
                    warn!(
                        target: "relay::client",
                        relay=name.to_string(),
                        ?err,
                        "Failed to get internal best bid",
                    );
                    None
                }
            },
            Err(err) => {
                warn!(
                    target: "relay::client",
                    relay=name.to_string(),
                    ?err,
                    "Failed to get internal best bid",
                );
                None
            }
        }
    }
}

#[derive(Default)]
pub struct SubmitBidResp {
    pub status: StatusCode,
    pub message: String,
}

/// Abstraction on https://flashbots.github.io/relay-specs/#/ Apis
#[async_trait::async_trait]
pub trait Relay: Send + Sync + Debug + 'static {
    /// Get relay info field
    fn info(&self) -> RelayInfo;

    /// Returns an array of validators registrations for the current and next epoch.
    /// /relay/v1/builder/validators
    async fn get_validators(&self) -> Vec<ValidatorRegistrationResponse>;

    /// Submit a new block to the relay.
    /// Any new submission by a builder will overwrite a previous one by the same builder_pubkey, even if it is less profitable.
    /// /relay/v1/builder/blocks
    async fn submit_bid(&self, bid: Arc<SignedBid>) -> Result<SubmitBidResp, RelayClientErr>;

    /// Retrieves all bid traces for a given block_number
    /// /relay/v1/data/bidtraces/builder_blocks_received
    async fn get_bid_traces(&self, block_number: u64) -> Option<(RelayName, Vec<BidTrace>)>;

    /// Retrieves best bid for a given block_number for internal relay
    async fn get_internal_best_bid(&self, block_number: u64, timestamp: u64) -> Option<SignedBid>;
}

