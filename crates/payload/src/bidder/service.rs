use builder_primitives::bid::{Bid, BidTrace, SignedBid};
use builder_primitives::build::Build;
use builder_primitives::signer::BlsSigner;
use consensus_layer::clock::NetworkClock;
use futures_core::Future;
use futures_util::FutureExt;
use relay::services::relay::{RelayHandle, RelayServiceErr};
use reth::providers::BlockReaderIdExt;
use reth::rpc::types::engine::ExecutionPayloadEnvelopeV3;
use reth::{
    primitives::{ChainSpec, U256},
    providers::StateProviderFactory,
    tasks::TaskSpawner,
    transaction_pool::TransactionPool,
};
use reth_payload_builder::error::PayloadBuilderError;
use std::time::{Duration, Instant};
use std::{
    cmp::Ordering,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use thiserror::Error;
use tokio::sync::mpsc::{self, error::SendError};
use tokio::sync::oneshot;
use tracing::{debug, error, trace};

use crate::strategy::state::{BuildState, BuildStateExecutionError};

use super::metrics::PayloadBidderMetrics;
use super::policy::{AuctionBidInfo, BidSelectionPolicy};

/// Bidder Service Error
#[derive(Debug, Error)]
pub enum BidderServiceError {
    /// Error sending build state to service
    #[error("Error sending build state to service: {0}")]
    SendState(#[from] SendError<Arc<BuildState>>),
    /// Unexpected execution result
    #[error("Unexpected execution result: {0}")]
    UnexpectedExecutionResult(String),
    /// Finalize build state into build error
    #[error("Failed to finalize build state into build: {0}")]
    Finalize(String),
    /// Payload builder error
    #[error("{0}")]
    PayloadBuilder(#[from] PayloadBuilderError),
    /// Build state execution error
    #[error("{0}")]
    BuilderPayment(#[from] BuildStateExecutionError),
}

/// A communication channel to the [BidderService].
#[derive(Debug, Clone)]
pub struct BidderServiceHandle {
    /// Build state sender half of the message channel to the [PayloadBuilderService].
    builds_sender: mpsc::UnboundedSender<Arc<BuildState>>,
    auction_start_sender: mpsc::UnboundedSender<(u64, u64)>,
}

impl BidderServiceHandle {
    /// Create new bidder service handle
    pub fn new(
        builds_sender: mpsc::UnboundedSender<Arc<BuildState>>,
        auction_start_sender: mpsc::UnboundedSender<(u64, u64)>,
    ) -> Self {
        Self {
            builds_sender,
            auction_start_sender,
        }
    }

    /// Send build state to bidder service
    pub fn make_bid(&self, state: Arc<BuildState>) -> Result<(), BidderServiceError> {
        self.builds_sender.send(state)?;
        Ok(())
    }

    /// Send build context to bidder service
    pub fn on_auction_start(&self, slot: u64, block_number: u64) {
        let _ = self.auction_start_sender.send((slot, block_number));
    }
}

/// Config struct for Bidder Service
pub struct BidderServiceConfig<Policy: BidSelectionPolicy> {
    /// The chain spec.
    pub chain_spec: Arc<ChainSpec>,
    /// Bid policy
    pub policy: Policy,
    /// The relay handle sends bid to the relay service
    pub relay_handle: RelayHandle,
    /// The signer used to sign bids
    pub signer: BlsSigner,
}

/// Bidder Service that receives build states from build jobs
///
/// Responsible for computing bid using [`BidSelectionPolicy`]
/// Service packages build state into a final build and submits to relay service
pub struct BidderService<Client, Pool, Tasks, Policy> {
    /// The client that can interact with the chain.
    client: Client,
    /// The transaction pool.
    pool: Pool,
    /// Task executor
    executor: Tasks,
    /// Handle for relay service
    relay_handle: RelayHandle,
    /// Build state receiver channel
    builds_receiver: mpsc::UnboundedReceiver<Arc<BuildState>>,
    auction_start_receiver: mpsc::UnboundedReceiver<(u64, u64)>,
    /// Auction top bid channel
    top_bid: Option<oneshot::Receiver<Result<Option<BidTrace>, RelayServiceErr>>>,
    /// Bid policy used for bidding
    policy: Policy,
    /// The network clock defines the timing for bidding for a slot auction
    network_clock: NetworkClock,
    /// Current slot bid state
    auction_bid_info: Option<AuctionBidInfo>,
    /// Bidder metrics
    metrics: PayloadBidderMetrics,
    /// The signer used to sign bids
    signer: BlsSigner,
}

impl<Client, Pool, Tasks, Policy> BidderService<Client, Pool, Tasks, Policy>
where
    Client: StateProviderFactory + BlockReaderIdExt + Unpin + Clone + 'static,
    Pool: TransactionPool + Unpin + 'static,
    Tasks: TaskSpawner + Clone + 'static,
    Policy: BidSelectionPolicy + Unpin + Clone,
{
    /// Create new bidder service future and handle
    pub fn new(
        client: Client,
        pool: Pool,
        executor: Tasks,
        config: BidderServiceConfig<Policy>,
    ) -> (Self, BidderServiceHandle) {
        let BidderServiceConfig {
            chain_spec,
            policy,
            relay_handle,
            signer,
        } = config;
        let (builds_sender, builds_receiver) = mpsc::unbounded_channel();
        let (auction_start_sender, auction_start_receiver) = mpsc::unbounded_channel();
        let handle = BidderServiceHandle::new(builds_sender, auction_start_sender);
        let network_clock = NetworkClock::try_from_chain(chain_spec.chain)
            .expect("unsupported chain for NetworkClock");
        let service = Self {
            client,
            pool,
            executor,
            relay_handle,
            builds_receiver,
            auction_start_receiver,
            top_bid: None,
            policy,
            network_clock,
            auction_bid_info: None,
            metrics: PayloadBidderMetrics::default(),
            signer,
        };
        (service, handle)
    }

    /// Initialize auction bid info on new slot auction
    pub fn on_new_slot_auction(&mut self, slot: u64, block_number: u64) {
        self.auction_bid_info = Some(AuctionBidInfo::new(slot, block_number));
        debug!(target: "payload::bidder::service", ?slot, ?block_number, "Slot auction start.");
    }

    /// Create a new process to request top bid in auction
    pub fn spawn_top_bid_request(&mut self) {
        if let Some(info) = self.auction_bid_info.as_ref() {
            let block_number = info.block_number();
            let relay_handle = self.relay_handle.clone();
            let (tx, rx) = oneshot::channel();
            self.executor.spawn(Box::pin(async move {
                let top_bid = relay_handle.get_top_bid(block_number).await;
                let _ = tx.send(top_bid);
            }));
            self.top_bid = Some(rx);
        }
    }
}

impl<Client, Pool, Tasks, Policy> Future for BidderService<Client, Pool, Tasks, Policy>
where
    Client: StateProviderFactory + BlockReaderIdExt + Unpin + Clone + 'static,
    Pool: TransactionPool + Unpin + 'static,
    Tasks: TaskSpawner + Clone + 'static,
    Policy: BidSelectionPolicy + Unpin + Clone + std::fmt::Debug,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        // triggers the start of a new auction
        while let Poll::Ready(msg) = this.auction_start_receiver.poll_recv(cx) {
            match msg {
                Some((slot, block_number)) => {
                    if this
                        .auction_bid_info
                        .as_ref()
                        .is_none_or(|info| info.slot() < slot)
                    {
                        this.metrics.inc_auction_infos();
                        this.on_new_slot_auction(slot, block_number);
                    }
                }
                None => {
                    error!(target: "payload::bidder::service", "Bidder service terminated");
                    return Poll::Ready(());
                }
            }
        }

        // continously poll for top bid in auction
        match this.top_bid.take() {
            Some(mut top_bid_rx) => match top_bid_rx.poll_unpin(cx) {
                Poll::Ready(top_bid_result) => {
                    match top_bid_result {
                        Ok(Ok(Some(top_bid))) => {
                            this.metrics.inc_auction_top_bids();
                            if let Some(info) = this.auction_bid_info.as_mut() {
                                info.update_auction_bid_info(*top_bid.value.as_ref());
                            }
                        }
                        Ok(Ok(None)) => {
                            // warn!(target: "payload::bidder::service", "Auction top bid not found");
                        }
                        _ => {}
                    }
                    this.spawn_top_bid_request();
                }
                Poll::Pending => {
                    this.top_bid = Some(top_bid_rx);
                }
            },
            None => {
                this.spawn_top_bid_request();
            }
        }

        // compare new and best builds, keeping best builds inside auction bid info
        loop {
            match this.builds_receiver.poll_recv(cx) {
                Poll::Ready(Some(build_state)) => {
                    let slot = build_state.build_config().slot;

                    // verify received build state matches current slot auction
                    let mut auction_bid_info = match this.auction_bid_info.take() {
                        Some(info) => match slot.cmp(&info.slot()) {
                            Ordering::Less => {
                                debug!(target: "payload::bidder::service", build_state=build_state.to_log(),  "Skipping build state with previous slot");
                                this.auction_bid_info = Some(info);
                                continue;
                            }
                            Ordering::Greater => {
                                debug!(target: "payload::bidder::service", build_state=build_state.to_log(), "Skipping build state with future slot");
                                this.auction_bid_info = Some(info);
                                continue;
                            }
                            Ordering::Equal => info,
                        },
                        None => {
                            debug!(target: "payload::bidder::service", build_state=build_state.to_log(), "Skipping build state, no auction bid info set");
                            continue;
                        }
                    };

                    let is_better = auction_bid_info
                        .best_block()
                        .as_ref()
                        .map(|block| {
                            this.policy.is_better_block_value(
                                block.exec_details(),
                                build_state.exec_details(),
                            )
                        })
                        .unwrap_or(true);

                    // verify build state has better block value
                    if !is_better {
                        debug!(target: "payload::bidder::service", build_state=build_state.to_log(), "Skipping build state with lower block value");
                        this.auction_bid_info = Some(auction_bid_info);
                        continue;
                    }

                    // update block info
                    let block_value = this.policy.block_value(build_state.exec_details());
                    auction_bid_info.update_block_info(build_state.clone(), block_value);
                    this.auction_bid_info = Some(auction_bid_info);
                }
                Poll::Ready(None) => {
                    error!(target: "payload::bidder::service", "Bidder service terminated");
                    return Poll::Ready(());
                }
                Poll::Pending => {
                    break;
                }
            }
        }

        // takes the best bid from auction bid info
        // computes it and checks if it's time to bid
        // if it is time to bid, it finalizes the build and submits to relay
        if let Some(mut auction_bid_info) = this.auction_bid_info.take() {
            // update time in auction
            let time_left = this
                .network_clock
                .time_until_slot_signed_ms(auction_bid_info.slot());
            auction_bid_info.refresh_time(time_left);

            if let (Some(bid_value), Some(build_state)) = (
                this.policy.compute_bid(&auction_bid_info),
                auction_bid_info.best_block(),
            ) {
                auction_bid_info.update_bid_info(bid_value);
                debug!(target: "payload::bidder::service", build_state=build_state.to_log(), ?bid_value, "New block submission ready for bidding");

                let client = this.client.clone();
                let pool = this.pool.clone();
                let relay_handle = this.relay_handle.clone();
                let metrics = this.metrics.clone();
                let signer = this.signer.clone();
                this.executor.spawn(Box::pin(async move {
                    finalize_and_submit_build(
                        client,
                        pool,
                        relay_handle,
                        build_state,
                        bid_value,
                        metrics,
                        signer,
                    )
                    .await;
                }));
            }
            this.auction_bid_info = Some(auction_bid_info);
        }

        Poll::Pending
    }
}

/// Add builder payment transaction to build state and package into final build
pub fn build_submission_with_builder_payment<Client, Pool>(
    client: Client,
    pool: Pool,
    mut build_state: BuildState,
    bid: U256,
) -> Result<(Build, Instant, Duration), BidderServiceError>
where
    Client: StateProviderFactory + Unpin + Clone + 'static,
    Pool: TransactionPool + Unpin + 'static,
{
    let build_ts = build_state.timestamp();
    let finalize_build_start = Instant::now();

    build_state.with_payment(bid);
    let build = build_state
        .into_build(client, pool)
        .map_err(|err| BidderServiceError::Finalize(err.to_string()))?;
    let elapsed = build_ts.elapsed();
    let txs = build.payload.block().body.len();
    trace!(target: "payload::service", ?elapsed, ?txs, "Build done.");

    Ok((build, build_ts, finalize_build_start.elapsed()))
}

/// Finalize and submit block by adding builder payment transaction to build state
/// and submitting to relay service via relay handle
pub async fn finalize_and_submit_build<Client, Pool>(
    client: Client,
    pool: Pool,
    relay_handle: RelayHandle,
    build_state: Arc<BuildState>,
    bid: U256,
    metrics: PayloadBidderMetrics,
    signer: BlsSigner,
) where
    Client: StateProviderFactory + Unpin + Clone + 'static,
    Pool: TransactionPool + Unpin + 'static,
{
    metrics.inc_initiated_finalized_builds();
    let block_number = build_state.build_config().block_number;
    let slot = build_state.build_config().slot;

    let build_result =
        build_submission_with_builder_payment(client, pool, BuildState::from(build_state), bid);
    match build_result {
        Ok((build, build_ts, finalize_latency)) => {
            metrics.inc_successful_finalized_builds();
            metrics.record_finalize_build_latency(finalize_latency);
            metrics.record_to_finalize_build_latency(build_ts.elapsed());
            metrics.inc_initiated_bids();

            debug!(
                target: "payload::bidder::service",
                ?build,
                "Submitting bid to relays",
            );

            let bid_start = Instant::now();
            let bid = Bid::new(&build, signer.public_key());
            let v3_envelope = ExecutionPayloadEnvelopeV3::from(build.payload);

            let signed_bid = Arc::new(SignedBid::new(bid, v3_envelope, signer));

            match relay_handle.submit_bid(signed_bid).await {
                Ok(_) => {
                    debug!(
                        target: "payload::bidder::service",
                        ?block_number,
                        ?slot,
                        "Submitted bid to relays",
                    );
                    metrics.inc_successful_bids();
                    metrics.record_bid_latency(bid_start.elapsed());
                    metrics.record_to_bid_latency(build_ts.elapsed());
                }
                Err(err) => {
                    error!(
                        target: "payload::bidder::service",
                        ?block_number,
                        ?slot,
                        ?err,
                        "Failed to submit bid",
                    );
                    metrics.inc_failed_bids();
                }
            }
        }
        Err(err) => {
            error!(target: "payload::bidder::service", ?err, "Failed to construct build from state");
            metrics.inc_failed_finalized_builds();
        }
    }
}
