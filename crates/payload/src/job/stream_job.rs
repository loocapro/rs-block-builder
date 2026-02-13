use crate::bidder::service::BidderServiceHandle;
use crate::strategy::state::BuildState;
use crate::strategy::{BuildArguments, BuildOutcome, Strategy};
use crate::{
    job::build_utils::{Cancelled, PayloadTaskGuard},
    traits::PayloadJob,
};
use builder_primitives::build::Build;
use builder_primitives::payload::BuildConfig;
use bundles::pool::BundlePool;
use futures_util::{Future, FutureExt};
use reth::{
    providers::StateProviderFactory, tasks::TaskSpawner, transaction_pool::TransactionPool,
};
use reth_payload_builder::error::PayloadBuilderError;
use reth_payload_builder::{
    database::CachedReads, EthPayloadBuilderAttributes, KeepPayloadJobAlive,
};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{sync::oneshot, time::Sleep};
use tracing::{debug, error, trace};

use super::build_utils::{PendingBuild, ResolveBestPayload};
use super::metrics::PayloadBuilderMetrics;

/// A build job that continuously constructs build states (unpackaged payloads)
/// The [`StreamBuildJob`] is responsible for sending completed build states to the bidder service
#[derive(Debug)]
pub struct StreamBuildJob<Client, Pool, Tasks> {
    /// Dynamic build configuration that changes on every slot
    pub build_config: BuildConfig,
    /// The client that can interact with the chain.
    pub client: Client,
    /// The transaction pool.
    pub pool: Pool,
    /// The bundle pool.
    pub bundle_pool: BundlePool,
    /// How to spawn building tasks
    pub executor: Tasks,
    /// Handle for bidder service
    pub bidder_handle: BidderServiceHandle,
    /// The deadline when this job should resolve.
    pub deadline: Pin<Box<Sleep>>,
    /// The best build so far.
    pub best_build: Option<Arc<BuildState>>,
    /// Receiver for the block that is currently being built.
    pub pending_build: Option<PendingBuild>,
    /// Restricts how many generator tasks can be executed at once.
    pub payload_task_guard: PayloadTaskGuard,
    /// Caches all disk reads for the state the new payloads builds on
    ///
    /// This is used to avoid reading the same state over and over again when new attempts are
    /// triggerd, because during the building process we'll repeatedly execute the transactions.
    pub cached_reads: Option<CachedReads>,
    /// Metrics for this type
    pub metrics: PayloadBuilderMetrics,
    /// The type responsible for building payloads.
    ///
    /// See [BuildStrategy]
    pub strategy: Strategy,
}

impl<Client, Pool, Tasks> Future for StreamBuildJob<Client, Pool, Tasks>
where
    Client: StateProviderFactory + Clone + Unpin + 'static,
    Pool: TransactionPool + Unpin + 'static,
    Tasks: TaskSpawner + Clone + 'static,
{
    type Output = Result<(), PayloadBuilderError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        // check if the deadline is reached
        if this.deadline.as_mut().poll(cx).is_ready() {
            debug!(target: "payload::job::stream", "Payload building deadline reached");
            return Poll::Ready(Ok(()));
        }

        // start a new job if there is no pending block and we haven't reached the deadline
        if this.pending_build.is_none() {
            trace!(target: "payload::job::stream", "Spawn new payload build task");
            let (tx, rx) = oneshot::channel();
            let client = this.client.clone();
            let pool = this.pool.clone();
            let bundle_pool = this.bundle_pool.clone();
            let cancel = Cancelled::default();
            let _cancel = cancel.clone();
            let guard = this.payload_task_guard.clone();
            let build_config = this.build_config.clone();
            let best_build = this.best_build.clone();
            this.metrics.inc_initiated_payload_builds();
            let cached_reads = this.cached_reads.take().unwrap_or_default();
            let strategy = this.strategy.clone();
            this.executor.spawn_blocking(Box::pin(async move {
                // acquire the permit for executing the task
                let _permit = guard.0.acquire().await;
                let args = BuildArguments {
                    client: client.clone(),
                    pool: pool.clone(),
                    bundle_pool: bundle_pool.clone(),
                    cached_reads,
                    build_config,
                    cancel,
                    best_build,
                };
                let result = strategy.try_build(args);
                let _ = tx.send(result);
            }));
            this.pending_build = Some(PendingBuild { _cancel, build: rx });
        }
        let build_config = &this.build_config;

        // poll the pending build
        if let Some(mut fut) = this.pending_build.take() {
            match fut.poll_unpin(cx) {
                Poll::Ready(Ok(outcome)) => {
                    match outcome {
                        BuildOutcome::Better {
                            build_state,
                            cached_reads,
                        } => {
                            debug!(target: "payload::job::stream", build_state=build_state.to_log(), "Better build state created");

                            this.metrics.inc_successful_payload_builds();
                            this.cached_reads = Some(cached_reads);
                            this.best_build = Some(build_state.clone());

                            if let Err(err) = this.bidder_handle.make_bid(build_state.clone()) {
                                error!(target: "payload::job::stream", build_state=build_state.to_log(), ?err, "Failed to send build state to bidder service");
                            }
                        }
                        BuildOutcome::Aborted {
                            block_value,
                            cached_reads,
                        } => {
                            trace!(target: "payload::job::stream", build_config=build_config.to_log(), ?block_value, "Skipping build state of worse build");
                            this.cached_reads = Some(cached_reads);
                        }
                        BuildOutcome::Cancelled => {
                            unreachable!("the cancel signal never fired")
                        }
                    }

                    // immediately wake up future
                    cx.waker().wake_by_ref();
                }
                Poll::Ready(Err(err)) => {
                    // job failed, but we simply try again
                    error!(target: "payload::job::stream", build_config=build_config.to_log(), ?err, "Build state attempt failed");
                    this.metrics.inc_failed_payload_builds();
                }
                Poll::Pending => {
                    this.pending_build = Some(fut);
                }
            }
        }

        Poll::Pending
    }
}

impl<Client, Pool, Tasks> PayloadJob for StreamBuildJob<Client, Pool, Tasks>
where
    Client: StateProviderFactory + Clone + Unpin + 'static,
    Pool: TransactionPool + Unpin + 'static,
    Tasks: TaskSpawner + Clone + 'static,
{
    type ResolvePayloadFuture = ResolveBestPayload;

    fn best_payload(&self) -> Result<Arc<Build>, PayloadBuilderError> {
        unimplemented!()
    }

    fn payload_attributes(&self) -> Result<EthPayloadBuilderAttributes, PayloadBuilderError> {
        unimplemented!()
    }

    fn resolve(&mut self) -> (Self::ResolvePayloadFuture, KeepPayloadJobAlive) {
        unimplemented!()
    }
}
