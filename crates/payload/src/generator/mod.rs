use crate::strategy::bundle::strat::BundlesStrat;
use crate::{
    bidder::service::BidderServiceHandle,
    job::{
        build_utils::PayloadTaskGuard, metrics::PayloadBuilderMetrics, stream_job::StreamBuildJob,
    },
    traits::PayloadJobGenerator,
};
use builder_primitives::payload::BuildConfig;
use bundles::pool::BundlePool;
use reth::{
    providers::{BlockReaderIdExt, StateProviderFactory},
    tasks::TaskSpawner,
    transaction_pool::TransactionPool,
};
use reth_payload_builder::error::PayloadBuilderError;

/// config for generator to build new jobs
pub mod config;
/// empty generator
pub mod empty;

use crate::generator::config::JobGeneratorConfig;

/// The [`StreamJobGenerator`] that creates [`StreamBuildJob`]s.
#[derive(Debug)]
pub struct StreamJobGenerator<Client, Pool, Tasks> {
    /// The client that can interact with the chain.
    client: Client,
    /// txpool
    pool: Pool,
    /// bundle pool
    bundle_pool: BundlePool,
    /// How to spawn building tasks
    executor: Tasks,
    /// The configuration for the job generator.
    pub config: JobGeneratorConfig,
    /// Restricts how many generator tasks can be executed at once.
    payload_task_guard: PayloadTaskGuard,
    /// Handle for bidder service
    bidder_handle: BidderServiceHandle,
}

// === impl StreamJobGenerator ===

impl<Client, Pool, Tasks> StreamJobGenerator<Client, Pool, Tasks>
where
    Client: StateProviderFactory + BlockReaderIdExt + Clone + Unpin + 'static,
{
    /// Creates a new [StreamJobGenerator] with the given config.
    pub fn new(
        client: Client,
        pool: Pool,
        bundle_pool: BundlePool,
        executor: Tasks,
        config: JobGeneratorConfig,
        bidder_handle: BidderServiceHandle,
    ) -> Self {
        Self {
            client,
            pool,
            bundle_pool,
            executor,
            payload_task_guard: PayloadTaskGuard::new(config.max_payload_tasks),
            config,
            bidder_handle,
        }
    }
}

// === impl PayloadJobGenerator ===

impl<Client, Pool, Tasks> PayloadJobGenerator for StreamJobGenerator<Client, Pool, Tasks>
where
    Client: StateProviderFactory + BlockReaderIdExt + Clone + Unpin + 'static,
    Pool: TransactionPool + Unpin + 'static,
    Tasks: TaskSpawner + Clone + Unpin + 'static,
{
    type Job = StreamBuildJob<Client, Pool, Tasks>;

    fn new_payload_job(
        &self,
        build_config: BuildConfig,
    ) -> Result<StreamBuildJob<Client, Pool, Tasks>, PayloadBuilderError> {
        let until = tokio::time::Instant::now() + self.config.deadline;
        let deadline = Box::pin(tokio::time::sleep_until(until));

        let job = StreamBuildJob {
            build_config,
            client: self.client.clone(),
            pool: self.pool.clone(),
            bundle_pool: self.bundle_pool.clone(),
            executor: self.executor.clone(),
            bidder_handle: self.bidder_handle.clone(),
            deadline,
            best_build: None,
            pending_build: None,
            cached_reads: None,
            metrics: PayloadBuilderMetrics::default(),
            payload_task_guard: self.payload_task_guard.clone(),
            strategy: BundlesStrat {
                config: self.config.payload_config.clone(),
            }
            .into(),
        };

        Ok(job)
    }
}

#[cfg(test)]
mod tests {
    use reth::{
        primitives::Withdrawals, tasks::TokioTaskExecutor,
        transaction_pool::noop::NoopTransactionPool,
    };
    use reth_payload_builder::{EthPayloadBuilderAttributes, PayloadId};
    use revm_primitives::{Address, FixedBytes};
    use std::time::Duration;
    use tokio::sync::mpsc;

    use crate::test_utils::mock_provider::MockProvider;

    struct TestPayloadBuilderAttributes(EthPayloadBuilderAttributes);

    impl AsRef<EthPayloadBuilderAttributes> for TestPayloadBuilderAttributes {
        fn as_ref(&self) -> &EthPayloadBuilderAttributes {
            &self.0
        }
    }

    impl Default for TestPayloadBuilderAttributes {
        fn default() -> Self {
            Self(EthPayloadBuilderAttributes {
                id: PayloadId::new([0; 8]),
                parent: Default::default(),
                timestamp: 0,
                suggested_fee_recipient: Address::ZERO,
                prev_randao: FixedBytes::default(),
                withdrawals: Withdrawals::new(vec![]),
                parent_beacon_block_root: None,
            })
        }
    }

    #[tokio::test]
    async fn can_create_empty_job() {
        use super::*;

        let client = MockProvider::default();
        let pool = NoopTransactionPool::default();
        let executor = TokioTaskExecutor::default();
        let config = JobGeneratorConfig::default()
            .interval(Duration::from_secs(2))
            .deadline(Duration::from_secs(6))
            .max_payload_tasks(1);
        let bundle_pool = BundlePool::new();
        let (state_tx, _) = mpsc::unbounded_channel();
        let (auction_tx, _) = mpsc::unbounded_channel();
        let bidder_handle = BidderServiceHandle::new(state_tx, auction_tx);
        let generator =
            StreamJobGenerator::new(client, pool, bundle_pool, executor, config, bidder_handle);
        let binding = TestPayloadBuilderAttributes::default();
        let attrs = binding.as_ref().clone();
        let config = BuildConfig::from(attrs.clone());

        let job = PayloadJobGenerator::new_payload_job(&generator, config).unwrap();

        assert_eq!(job.build_config.attributes.clone(), attrs);
    }
}
