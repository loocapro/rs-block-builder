use crate::strategy::empty::build_empty_payload;
use crate::strategy::state::BuildState;
use crate::strategy::{BuildArguments, BuildOutcome, Strategy};
use crate::{
    job::{
        build_utils::{Cancelled, PayloadTaskGuard, PendingBuild},
        metrics::PayloadBuilderMetrics,
    },
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
use tokio::{
    sync::oneshot,
    time::{Interval, Sleep},
};
use tracing::{debug, error, trace};

use super::build_utils::ResolveBestPayload;

/// A build job that on every interval builds a payload with the best transactions from the pool.
#[derive(Debug)]
pub struct IntervalBuildJob<Client, Pool, Tasks> {
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
    /// The deadline when this job should resolve.
    pub deadline: Pin<Box<Sleep>>,
    /// The interval at which the job should build a new payload after the last.
    pub interval: Interval,
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
    /// metrics for this type
    pub metrics: PayloadBuilderMetrics,
    /// The type responsible for building payloads.
    ///
    /// See [BuildStrategy]
    pub strategy: Strategy,
}

impl<Client, Pool, Tasks> Future for IntervalBuildJob<Client, Pool, Tasks>
where
    Client: StateProviderFactory + Clone + Unpin + 'static,
    Pool: TransactionPool + Unpin + 'static,
    Tasks: TaskSpawner + Clone + 'static,
{
    type Output = Result<(), PayloadBuilderError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let build_config = &this.build_config;

        // check if the deadline is reached
        if this.deadline.as_mut().poll(cx).is_ready() {
            debug!(target: "payload::job::interval", build_config=build_config.to_log(), "Payload building deadline reached");
            return Poll::Ready(Ok(()));
        }

        // check if the interval is reached
        while this.interval.poll_tick(cx).is_ready() {
            // start a new job if there is no pending block and we haven't reached the deadline
            if this.pending_build.is_none() {
                trace!(target: "payload::job::interval", build_config=build_config.to_log(),  "Spawn new payload build task");
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
        }
        let build_config = &this.build_config;

        // poll the pending block
        if let Some(mut fut) = this.pending_build.take() {
            match fut.poll_unpin(cx) {
                Poll::Ready(Ok(outcome)) => {
                    this.interval.reset();
                    this.metrics.inc_successful_payload_builds();
                    match outcome {
                        BuildOutcome::Better {
                            build_state,
                            cached_reads,
                        } => {
                            debug!(target: "payload::job::interval", build_state=build_state.to_log(), "Better build state created");

                            this.cached_reads = Some(cached_reads);
                            this.best_build = Some(build_state);
                        }
                        BuildOutcome::Aborted {
                            block_value,
                            cached_reads,
                        } => {
                            trace!(target: "payload::job::interval", build_config=build_config.to_log(), ?block_value, "Skipping build state of worse build");
                            this.cached_reads = Some(cached_reads);
                        }
                        BuildOutcome::Cancelled => {
                            unreachable!("the cancel signal never fired")
                        }
                    }
                }
                Poll::Ready(Err(err)) => {
                    // job failed, but we simply try again next interval
                    error!(target: "payload::job::interval", build_config=build_config.to_log(), ?err, "Payload build attempt failed");
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

impl<Client, Pool, Tasks> PayloadJob for IntervalBuildJob<Client, Pool, Tasks>
where
    Client: StateProviderFactory + Clone + Unpin + 'static,
    Pool: TransactionPool + Unpin + 'static,
    Tasks: TaskSpawner + Clone + 'static,
{
    type ResolvePayloadFuture = ResolveBestPayload;

    fn best_payload(&self) -> Result<Arc<Build>, PayloadBuilderError> {
        if let Some(best_build) = &self.best_build {
            let build_state = Arc::clone(best_build);
            return Ok(BuildState::from(build_state)
                .into_empty_build(self.client.clone(), self.pool.clone())?
                .into());
        }
        // No payload has been built yet, but we need to return something that the CL then can
        // deliver, so we need to return an empty payload.
        //
        // Note: it is assumed that this is unlikely to happen, as the payload job is started right
        // away and the first full block should have been built by the time CL is requesting the
        // payload.
        let args = BuildArguments {
            client: self.client.clone(),
            pool: self.pool.clone(),
            bundle_pool: self.bundle_pool.clone(),
            cached_reads: CachedReads::default(),
            build_config: self.build_config.clone(),
            cancel: Cancelled::default(),
            best_build: None,
        };
        build_empty_payload(args).map(Arc::new)
    }

    fn payload_attributes(&self) -> Result<EthPayloadBuilderAttributes, PayloadBuilderError> {
        Ok(self.build_config.attributes.clone())
    }

    fn resolve(&mut self) -> (Self::ResolvePayloadFuture, KeepPayloadJobAlive) {
        let mut empty_payload = None;

        let best_build = match self.best_build.take() {
            Some(build_state) => {
                if let Ok(build) = BuildState::from(build_state)
                    .into_empty_build(self.client.clone(), self.pool.clone())
                {
                    Some(Arc::new(build))
                } else {
                    None
                }
            }
            None => None,
        };

        if best_build.is_none() {
            // no payload built yet, so we need to return an empty payload
            let (tx, rx) = oneshot::channel();
            let build_config = self.build_config.clone();
            let client = self.client.clone();
            let pool = self.pool.clone();
            let bundle_pool = self.bundle_pool.clone();
            self.executor.spawn_blocking(Box::pin(async move {
                let args = BuildArguments {
                    client,
                    pool,
                    bundle_pool,
                    cached_reads: CachedReads::default(),
                    build_config,
                    cancel: Cancelled::default(),
                    best_build: None,
                };
                let res = build_empty_payload(args);
                let _ = tx.send(res);
            }));

            empty_payload = Some(rx);
        }

        let fut = ResolveBestPayload {
            best_payload: best_build,
            maybe_better: None,
            empty_payload,
        };

        (fut, KeepPayloadJobAlive::No)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bundles::pool::BundlePool;
    use tokio::time::{interval, sleep};

    use crate::strategy::mempool::MempoolStrat;
    use crate::{
        job::{
            build_utils::PayloadTaskGuard,
            interval_job::IntervalBuildJob,
            metrics::PayloadBuilderMetrics,
            tests::{add_tx_test_pool, setup_test_env, test_transaction, TestBuildJob},
        },
        strategy::empty::EmptyStrat,
    };

    #[tokio::test]
    async fn mempool() {
        let (build_config, client, task_manager, pool, _, payload_id) = setup_test_env();

        add_tx_test_pool(&pool, test_transaction(1, 0)).await;

        let job = TestBuildJob {
            inner: IntervalBuildJob {
                build_config,
                client,
                pool,
                bundle_pool: BundlePool::default(),
                executor: task_manager.executor(),
                deadline: Box::pin(sleep(Duration::from_secs(1))),
                interval: interval(Duration::from_secs(1)),
                best_build: None,
                pending_build: None,
                payload_task_guard: PayloadTaskGuard::new(1),
                cached_reads: None,
                metrics: PayloadBuilderMetrics::default(),
                strategy: MempoolStrat::default().into(),
            },
        };

        let result = job
            .await
            .expect("Job initial await failed")
            .await
            .expect("Job execution failed");

        assert_eq!(payload_id, result.payload.id());
        assert_eq!(result.payload.block().body.len(), 0);
    }

    #[tokio::test]
    async fn empty() {
        let (build_config, client, task_manager, pool, _, payload_id) = setup_test_env();

        let job = TestBuildJob {
            inner: IntervalBuildJob {
                build_config,
                client,
                pool,
                bundle_pool: BundlePool::default(),
                executor: task_manager.executor(),
                deadline: Box::pin(sleep(Duration::from_secs(1))),
                interval: interval(Duration::from_secs(1)),
                best_build: None,
                pending_build: None,
                payload_task_guard: PayloadTaskGuard::new(1),
                cached_reads: None,
                metrics: PayloadBuilderMetrics::default(),
                strategy: EmptyStrat::default().into(),
            },
        };

        let result = job
            .await
            .expect("Job initial await failed")
            .await
            .expect("Job execution failed");

        assert_eq!(payload_id, result.payload.id());
        assert_eq!(result.payload.block().body.len(), 0);
    }
}
