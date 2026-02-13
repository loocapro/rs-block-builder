use builder_primitives::build::Build;
use builder_primitives::payload::PayloadConfig;
use reth::providers::StateProviderFactory;
use reth::revm::{database::StateProviderDatabase, State};
use reth::transaction_pool::TransactionPool;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Instant;
use tracing::trace;

use crate::strategy::state::BuildState;
use crate::strategy::{BuildArguments, BuildOutcome, BuildStrategy};
use reth_payload_builder::error::PayloadBuilderError;

/// Empty Strategy that returns a valid block with no transactions
#[derive(Debug, Clone, Default)]
pub struct EmptyStrat {
    /// The configuration for how the payload will be created.
    pub config: PayloadConfig,
}

impl BuildStrategy for EmptyStrat {
    fn try_build<Client: StateProviderFactory + Clone, Pool: TransactionPool + Clone>(
        &self,
        args: BuildArguments<Client, Pool>,
    ) -> Result<BuildOutcome, PayloadBuilderError> {
        let BuildArguments {
            client,
            mut cached_reads,
            build_config,
            ..
        } = args;

        let state_provider = client.state_by_block_hash(build_config.parent_block.hash())?;
        let state = StateProviderDatabase::new(state_provider);
        let mut db = State::builder()
            .with_database_ref(cached_reads.as_db(&state))
            .with_bundle_update()
            .build();

        let mut build_state = BuildState::new(build_config, self.config.clone(), Instant::now());
        trace!(target: "payload::strategy::empty", build_state=build_state.to_log(), "Building new build state");

        // apply eip-4788 pre block contract call
        build_state.pre_block_beacon_root_contract_call(&mut db)?;

        Ok(BuildOutcome::Better {
            build_state: Arc::new(build_state),
            cached_reads,
        })
    }
}

/// Default implementation to build a payload with no transactions
pub fn build_empty_payload<Client, Pool>(
    args: BuildArguments<Client, Pool>,
) -> Result<Build, PayloadBuilderError>
where
    Client: StateProviderFactory + Clone,
    Pool: TransactionPool,
{
    let client = args.client.clone();
    let pool = args.pool.clone();
    match EmptyStrat::default().try_build(args) {
        Ok(BuildOutcome::Better { build_state, .. }) => {
            Ok(BuildState::from(build_state).into_empty_build(client, pool)?)
        }
        Err(err) => Err(err),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::build_utils::Cancelled;
    use crate::job::tests::setup_test_env;
    use bundles::pool::BundlePool;
    use reth_payload_builder::database::CachedReads;

    #[tokio::test]
    async fn build_empty() {
        let (build_config, client, _, pool, _, payload_id) = setup_test_env();
        let build_args = BuildArguments {
            client,
            pool,
            bundle_pool: BundlePool::default(),
            cached_reads: CachedReads::default(),
            build_config,
            cancel: Cancelled::default(),
            best_build: None,
        };

        let build =
            build_empty_payload(build_args).expect("Building empty payload should not fail");

        assert_eq!(payload_id, build.payload.id());
        assert_eq!(build.payload.block().body.len(), 0);
    }

    #[tokio::test]
    async fn empty_try_build() {
        let (build_config, client, _, pool, _, payload_id) = setup_test_env();
        let build_args = BuildArguments {
            client,
            pool,
            bundle_pool: BundlePool::default(),
            cached_reads: CachedReads::default(),
            build_config,
            cancel: Cancelled::default(),
            best_build: None,
        };

        let client = build_args.client;
        let pool = build_args.pool.clone();
        let strategy = EmptyStrat::default();
        if let Ok(BuildOutcome::Better { build_state, .. }) = strategy.try_build(build_args) {
            let build = BuildState::from(build_state)
                .into_empty_build(client, pool)
                .expect("Failed to convert build state into empty build");

            assert_eq!(payload_id, build.payload.id());
            assert_eq!(build.payload.block().body.len(), 0);
        } else {
            unreachable!("Expected a 'Better' build outcome or success without errors");
        }
    }
}
