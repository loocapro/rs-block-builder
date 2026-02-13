use crate::client::Relay;
use async_trait::async_trait;
use builder_primitives::bid::{BidTrace, SignedBid};
use builder_primitives::relays::{RelayInfo, RelayName};
use builder_primitives::validator::ValidatorRegistrationResponse;
use core::fmt::Debug;
use reqwest::Client;
use std::sync::Arc;

use std::time::Duration;
use tracing::{debug, error, warn};

use super::{
    RelayClientErr, SubmitBidResp, BID_TRACES_PATH, INTERNAL_BEST_BID_PATH,
    VALIDATOR_REGISTRATIONS_PATH,
};

#[derive(Debug, Clone)]
/// Simulation client can fetch info but does not submit bids
pub struct SimulateRelayClient {
    client: Client,
    info: RelayInfo,
}
impl SimulateRelayClient {
    pub fn new(relay: RelayInfo) -> Self {
        Self {
            client: Client::new(),
            info: relay,
        }
    }
}

#[async_trait]
impl Relay for SimulateRelayClient {
    /// Get relay info field
    fn info(&self) -> RelayInfo {
        self.info.clone()
    }

    /// Get list of validators from flashbots relay, this is required to construct a bid
    /// fails fast if response is slow
    async fn get_validators(&self) -> Vec<ValidatorRegistrationResponse> {
        let url = format!("{}{}", self.info.url(), VALIDATOR_REGISTRATIONS_PATH);
        let name = self.info.name();
        let request = self.client.get(url).timeout(Duration::from_secs(2));

        match request.send().await {
            Ok(resp) => match resp.json::<Vec<ValidatorRegistrationResponse>>().await {
                Ok(data) => {
                    debug!(target: "relay::client", relay=name.to_string(), "Loaded {} validators", data.len());
                    data
                }
                Err(err) => {
                    error!(target: "relay::client", relay=name.to_string(), ?err, "Failed to get validators");
                    vec![]
                }
            },
            Err(err) => {
                error!(target: "relay::client", relay=name.to_string(), ?err, "Failed to get validators");
                vec![]
            }
        }
    }
    /// Submit bid to the given relay
    async fn submit_bid(&self, _bid: Arc<SignedBid>) -> Result<SubmitBidResp, RelayClientErr> {
        Ok(SubmitBidResp::default())
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
        let name = self.info.name().clone();
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
