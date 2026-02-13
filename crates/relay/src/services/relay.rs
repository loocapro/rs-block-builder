use crate::aggregator::RelayAggregator;
use crate::client::RelayClientErr;
use crate::relay::RelayPool;
use builder_primitives::bid::{BidTrace, RelayBidTrace, SignedBid};
use builder_primitives::blst::public_key::BlsPublicKey;
use builder_primitives::run_mode::RunMode;
use builder_primitives::validator::ValidatorSchedule;
use futures_util::stream::StreamExt;
use std::sync::Arc;
use std::task::Context;
use std::{future::Future, pin::Pin, task::Poll};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::oneshot::error::RecvError;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::UnboundedReceiverStream;

/// List of relay commands
pub enum RelayCommands {
    /// SubmitBid command
    SubmitBid(Arc<SignedBid>, oneshot::Sender<SubmitBidFuture>),
    /// GetBidTraces command
    GetBidTraces(u64, oneshot::Sender<GetBidTracesFuture>),
    /// GetInternalBestBid command
    GetInternalBestBid(u64, u64, oneshot::Sender<GetInternalBestBidFuture>),
    /// Load validator schedule
    LoadValidatorSchedule(oneshot::Sender<LoadValidatorScheduleFuture>),
}

type SubmitBidFuture = Pin<Box<dyn Future<Output = Result<(), RelayClientErr>> + Send + 'static>>;

type GetBidTracesFuture = Pin<Box<dyn Future<Output = Vec<RelayBidTrace>> + Send + 'static>>;

/// GetInternalBestBid future
type GetInternalBestBidFuture = Pin<Box<dyn Future<Output = Option<SignedBid>> + Send + 'static>>;

/// LoadValidatorSchedule future
type LoadValidatorScheduleFuture =
    Pin<Box<dyn Future<Output = Result<ValidatorSchedule, RelayClientErr>> + Send + 'static>>;

#[derive(Debug, Error)]
pub enum RelayServiceErr {
    #[error("Could not recieve response")]
    Recv(#[from] RecvError),
    #[error("Could not send message")]
    Send(#[from] SendError<RelayCommands>),
    #[error("Relay Client Error")]
    Client(#[from] RelayClientErr),
}

/// RelayService takes care of handling async commands to the mev-boost relay api
pub struct RelayService {
    /// Comunication channel to receive and process commands
    command_rx: UnboundedReceiverStream<RelayCommands>,
    /// Aggregates relay interactions
    relay_aggregator: Arc<RelayAggregator>,
}

impl RelayService {
    pub fn new(
        run_mode: RunMode,
        internal_relay_enabled: bool,
        public_key: BlsPublicKey,
    ) -> (Self, RelayHandle) {
        let (to_service, command_rx) = mpsc::unbounded_channel();
        let command_rx = UnboundedReceiverStream::new(command_rx);

        let handle = RelayHandle {
            to_service,
            public_key,
        };

        let relay_aggregator = Arc::new(RelayAggregator::new(RelayPool::new(
            run_mode,
            internal_relay_enabled,
        )));

        let this = Self {
            command_rx,
            relay_aggregator,
        };
        (this, handle)
    }

    /// From a signed bid it returns a submit bid future
    fn submit_bid(&self, bid: Arc<SignedBid>) -> SubmitBidFuture {
        let relay_aggregator = self.relay_aggregator.clone();
        let fut = async move { relay_aggregator.broadcast_submit_bid(bid).await };
        Box::pin(fut)
    }

    pub fn get_bid_traces(&self, block_number: u64) -> GetBidTracesFuture {
        let relay_aggregator = self.relay_aggregator.clone();
        Box::pin(async move { relay_aggregator.bid_traces(block_number).await })
    }

    pub fn get_internal_best_bid(
        &self,
        block_number: u64,
        timestamp: u64,
    ) -> GetInternalBestBidFuture {
        let relay_aggregator = self.relay_aggregator.clone();
        Box::pin(async move {
            relay_aggregator
                .internal_best_bid(block_number, timestamp)
                .await
        })
    }

    pub fn load_validator_schedule(&self) -> LoadValidatorScheduleFuture {
        let relay_aggregator = self.relay_aggregator.clone();
        Box::pin(async move { relay_aggregator.load_validator_schedule().await })
    }
}

/// Main service loop
/// Handles different relay commands and send back the future to be processed to the RelayHandle
impl Future for RelayService {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        while let Poll::Ready(Some(cmd)) = this.command_rx.poll_next_unpin(cx) {
            match cmd {
                RelayCommands::SubmitBid(bid, tx) => {
                    let fut = this.submit_bid(bid);
                    let _ = tx.send(fut);
                }
                RelayCommands::GetBidTraces(block_number, tx) => {
                    let fut = this.get_bid_traces(block_number);
                    let _ = tx.send(fut);
                }
                RelayCommands::GetInternalBestBid(block_number, timestamp, tx) => {
                    let fut = this.get_internal_best_bid(block_number, timestamp);
                    let _ = tx.send(fut);
                }
                RelayCommands::LoadValidatorSchedule(tx) => {
                    let fut = this.load_validator_schedule();
                    let _ = tx.send(fut);
                }
            }
        }

        Poll::Pending
    }
}

#[derive(Debug, Clone)]
/// Relay handle to send commands to the service
pub struct RelayHandle {
    /// Main comunication channel to the relay service
    to_service: mpsc::UnboundedSender<RelayCommands>,
    /// Builder bls public key
    public_key: BlsPublicKey,
}

/// Main entry point of our relay service
impl RelayHandle {
    /// Submit a bid command to the relay service
    /// receieves a SubmitBitFuture and awaits it
    pub async fn submit_bid(&self, to_bid: Arc<SignedBid>) -> Result<(), RelayServiceErr> {
        let (tx, rx) = oneshot::channel::<SubmitBidFuture>();
        self.to_service.send(RelayCommands::SubmitBid(to_bid, tx))?;
        Ok(rx.await?.await?)
    }

    /// Receives bid traces for given slot
    pub async fn get_bid_traces(
        &self,
        block_number: u64,
    ) -> Result<Vec<RelayBidTrace>, RelayServiceErr> {
        let (tx, rx) = oneshot::channel::<GetBidTracesFuture>();
        self.to_service
            .send(RelayCommands::GetBidTraces(block_number, tx))?;
        Ok(rx.await?.await)
    }

    /// Receives bid traces for given slot
    pub async fn get_top_bid(
        &self,
        block_number: u64,
    ) -> Result<Option<BidTrace>, RelayServiceErr> {
        let (tx, rx) = oneshot::channel::<GetBidTracesFuture>();
        self.to_service
            .send(RelayCommands::GetBidTraces(block_number, tx))?;
        let top_bid = rx
            .await?
            .await
            .into_iter()
            .flat_map(|(_, traces)| traces)
            .filter(|t| !t.builder_pubkey.eq(&self.public_key))
            .max_by_key(|t| t.value);
        Ok(top_bid)
    }

    /// Receives bid traces for given slot
    pub async fn get_internal_best_bid(
        &self,
        block_number: u64,
        timestamp: u64,
    ) -> Result<Option<SignedBid>, RelayServiceErr> {
        let (tx, rx) = oneshot::channel::<GetInternalBestBidFuture>();
        self.to_service.send(RelayCommands::GetInternalBestBid(
            block_number,
            timestamp,
            tx,
        ))?;
        Ok(rx.await?.await)
    }

    /// Refresh cached validator schedule
    pub async fn load_validator_schedule(&self) -> Result<ValidatorSchedule, RelayServiceErr> {
        let (tx, rx) = oneshot::channel::<LoadValidatorScheduleFuture>();
        self.to_service
            .send(RelayCommands::LoadValidatorSchedule(tx))?;
        rx.await?.await.map_err(|err| err.into())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use builder_primitives::{
        bid::Bid,
        build::{Build, BuildId},
        signer::BlsSigner,
    };

    use reth::{
        primitives::{Block, U256},
        rpc::types::engine::ExecutionPayloadEnvelopeV3,
    };
    use reth_payload_builder::{EthBuiltPayload, PayloadId};

    use std::time::Duration;

    #[ignore]
    #[tokio::test]
    async fn can_submit_bid() {
        let (service, handle) =
            RelayService::new(RunMode::Simulate, false, BlsPublicKey::default());
        let bp = EthBuiltPayload::new(
            PayloadId::new([0; 8]),
            Block::default().seal_slow(),
            U256::ZERO,
        );
        let signer = BlsSigner::default();
        let build = Build {
            id: BuildId::random(),
            validator_info: Default::default(),
            payload: bp,
            bid: U256::ZERO,
        };
        tokio::spawn(service);

        tokio::time::sleep(Duration::from_millis(500)).await;
        let bid = Bid::new(&build, signer.public_key());
        let v3_envelope = ExecutionPayloadEnvelopeV3::from(build.payload);
        let signed_bid = Arc::new(SignedBid::new(bid, v3_envelope, signer));

        handle.submit_bid(signed_bid).await.unwrap();
    }
}
