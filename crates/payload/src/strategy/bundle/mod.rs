use std::sync::Arc;
use std::time::Instant;

use crate::job::metrics::PayloadBuilderMetrics;

use crate::strategy::bundle::strat::BundlesStrat;
use crate::strategy::bundle::strat::TxExecution;
use crate::strategy::{state::BuildState, BuildArguments, BuildOutcome, BuildStrategy};

use reth::providers::StateProviderFactory;
use reth::revm::{
    database::StateProviderDatabase, db::states::bundle_state::BundleRetention, State,
};
use reth::transaction_pool::BestTransactionsAttributes;
use reth::transaction_pool::TransactionPool;
use reth_payload_builder::error::PayloadBuilderError;
use revm_primitives::U256;
use tracing::trace;

/// MevBundles is a strategy that builds a payload by selecting and executing MEV bundles
pub mod strat;

impl BuildStrategy for BundlesStrat {
    fn try_build<Client: StateProviderFactory + Clone, Pool: TransactionPool + Clone>(
        &self,
        args: BuildArguments<Client, Pool>,
    ) -> Result<BuildOutcome, PayloadBuilderError> {
        let BuildArguments {
            client,
            pool,
            bundle_pool,
            mut cached_reads,
            build_config,
            cancel,
            best_build,
        } = args;

        let payload_config = self.config.clone();

        let build_start = Instant::now();

        let base_fee = build_config.initialized_block_env.basefee.to::<u64>();
        let blob_gas_price = build_config
            .initialized_block_env
            .get_blob_gasprice()
            .map(|gasprice| gasprice as u64);

        let best_tx_attributes = BestTransactionsAttributes::new(base_fee, blob_gas_price);

        let state_provider = client.state_by_block_hash(build_config.parent_block.hash())?;
        let state = StateProviderDatabase::new(state_provider);
        let mut db = State::builder()
            .with_database_ref(cached_reads.as_db(&state))
            .with_bundle_update()
            .build();

        let mut build_state = BuildState::new(build_config, payload_config, build_start);
        trace!(target: "payload::strategy::bundle", build_state = build_state.to_log(), "Building new build state");

        // apply eip-4788 pre block contract call
        build_state.pre_block_beacon_root_contract_call(&mut db)?;

        let processed_bundles =
            self.decode_and_filter_bundles(&mut db, &mut build_state, &bundle_pool)?;

        let mut committed_txs =
            self.commit_selected_bundles(&mut db, &mut build_state, processed_bundles)?;

        match self.execute_mempool_txs(
            &pool,
            &mut build_state,
            &mut db,
            &mut committed_txs,
            &cancel,
            best_tx_attributes,
        ) {
            Ok(()) => (),
            Err(TxExecution::JobCancelled) => return Ok(BuildOutcome::Cancelled),
            Err(TxExecution::ExecFailed) => {
                return Ok(BuildOutcome::Aborted {
                    block_value: build_state.block_value(),
                    cached_reads,
                });
            }
        }

        db.merge_transitions(BundleRetention::PlainState);
        build_state.with_bundle_state(db.take_bundle());

        let metrics = PayloadBuilderMetrics::default();
        metrics.record_payload_build_latency(build_start.elapsed());

        let current_block_value = build_state.block_value();
        let best_block_value = best_build.map_or(U256::ZERO, |best| best.block_value());

        if current_block_value <= best_block_value {
            Ok(BuildOutcome::Aborted {
                block_value: current_block_value,
                cached_reads,
            })
        } else {
            Ok(BuildOutcome::Better {
                build_state: Arc::new(build_state),
                cached_reads,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use bundles::pool::BundlePool;
    use reth::primitives::constants::GWEI_TO_WEI;
    use reth::primitives::{
        sign_message, Transaction, TransactionKind, TransactionSigned,
        TransactionSignedEcRecovered, TxLegacy, U256, B256,
    };
    use reth_payload_builder::database::CachedReads;
    use revm_primitives::bytes;
    use secp256k1::SecretKey;
    use std::str::FromStr;
    use std::sync::Arc;

    use crate::job::build_utils::Cancelled;
    use crate::job::tests::{
        add_test_bundle, add_tx_test_pool, setup_test_env, test_bundle, test_transaction,
    };
    use crate::strategy::BuildOutcome;

    use super::*;

    trait ExpectBetter {
        fn expect_better(self, msg: &str) -> Arc<BuildState>;
        fn expect_aborted(self, msg: &str);
    }

    impl ExpectBetter for Result<BuildOutcome, PayloadBuilderError> {
        fn expect_better(self, msg: &str) -> Arc<BuildState> {
            match self {
                Ok(BuildOutcome::Better { build_state, .. }) => build_state,
                _ => panic!("{}", msg),
            }
        }
        fn expect_aborted(self, msg: &str) {
            match self {
                Ok(BuildOutcome::Aborted { .. }) => (),
                _ => panic!("{}", msg),
            }
        }
    }

    #[tokio::test]
    async fn top_of_block() {
        let (build_config, client, _, pool, bundle_pool, payload_id) = setup_test_env();

        let build_args = BuildArguments {
            client,
            pool: pool.clone(),
            bundle_pool: bundle_pool.clone(),
            cached_reads: CachedReads::default(),
            build_config,
            cancel: Cancelled::default(),
            best_build: None,
        };

        let bundle_tx = test_transaction(1, 0);
        let mempool_tx = test_transaction(2, 0);
        add_tx_test_pool(&pool, mempool_tx.clone()).await;
        add_test_bundle(&bundle_pool, test_bundle(1, vec![bundle_tx.clone()]));

        let strategy = BundlesStrat::default();
        let build_state = strategy
            .try_build(build_args)
            .expect_better("Expected a Better build outcome");

        let build = BuildState::from(build_state)
            .into_build(client, pool)
            .expect("failed build");

        assert_eq!(payload_id, build.payload.id());
        assert_eq!(build.payload.block().body.len(), 3);
        assert_eq!(build.payload.block().body[0].hash(), bundle_tx.hash());
        assert_eq!(build.payload.block().body[1].hash(), mempool_tx.hash());
    }

    #[tokio::test]
    async fn no_bundles() {
        let (build_config, client, _, pool, _, payload_id) = setup_test_env();
        let build_args = BuildArguments {
            client,
            pool: pool.clone(),
            bundle_pool: BundlePool::default(),
            cached_reads: CachedReads::default(),
            build_config,
            cancel: Cancelled::default(),
            best_build: None,
        };

        add_tx_test_pool(&pool, test_transaction(1, 0)).await;
        add_tx_test_pool(&pool, test_transaction(3, 0)).await;
        add_tx_test_pool(&pool, test_transaction(2, 0)).await;

        let strategy = BundlesStrat::default();
        let build_state = strategy
            .try_build(build_args)
            .expect_better("Expected a Better build outcome");

        // Assuming `into_build` is synchronous. If it's async, await it accordingly.
        let build = BuildState::from(build_state)
            .into_build(client, pool)
            .expect("failed build");

        assert_eq!(payload_id, build.payload.id());
        assert_eq!(build.payload.block().body.len(), 4);
        assert_eq!(
            build.payload.block().body[0]
                .transaction
                .max_priority_fee_per_gas(),
            Some((3 * GWEI_TO_WEI).into()),
        );
        assert_eq!(
            build.payload.block().body[1]
                .transaction
                .max_priority_fee_per_gas(),
            Some((2 * GWEI_TO_WEI).into()),
        );
        assert_eq!(
            build.payload.block().body[2]
                .transaction
                .max_priority_fee_per_gas(),
            Some(GWEI_TO_WEI.into()),
        );
    }

    #[tokio::test]
    async fn order_by_prio() {
        let (build_config, client, _, pool, bundle_pool, payload_id) = setup_test_env();
        let build_args = BuildArguments {
            client,
            pool: pool.clone(),
            bundle_pool: bundle_pool.clone(),
            cached_reads: CachedReads::default(),
            build_config,
            cancel: Cancelled::default(),
            best_build: None,
        };

        let bundle_tx_1 = test_transaction(1, 0);
        add_test_bundle(&bundle_pool, test_bundle(1, vec![bundle_tx_1.clone()]));
        let bundle_tx_2 = test_transaction(2, 0);
        add_test_bundle(&bundle_pool, test_bundle(1, vec![bundle_tx_2.clone()]));

        let strategy = BundlesStrat::default();
        let build_state = strategy
            .try_build(build_args)
            .expect_better("Expected a Better build outcome");

        let build = BuildState::from(build_state)
            .into_build(client, pool)
            .expect("Failed to convert build state into build");

        assert_eq!(payload_id, build.payload.id());
        assert_eq!(build.payload.block().body.len(), 3);
        assert_eq!(build.payload.block().body[0].hash(), bundle_tx_2.hash());
        assert_eq!(build.payload.block().body[1].hash(), bundle_tx_1.hash());
    }

    #[tokio::test]
    async fn multiple_tx_bundle() {
        let (build_config, client, _, pool, bundle_pool, payload_id) = setup_test_env();

        let build_args = BuildArguments {
            client,
            pool: pool.clone(),
            bundle_pool: bundle_pool.clone(),
            cached_reads: CachedReads::default(),
            build_config,
            cancel: Cancelled::default(),
            best_build: None,
        };

        let bundle_tx_1 = test_transaction(4, 4);
        add_test_bundle(&bundle_pool, test_bundle(1, vec![bundle_tx_1.clone()]));
        let bundle_tx_2 = test_transaction(2, 3);
        let bundle_tx_3 = test_transaction(1, 3);
        add_test_bundle(
            &bundle_pool,
            test_bundle(1, vec![bundle_tx_2.clone(), bundle_tx_3.clone()]),
        );

        let strategy = BundlesStrat::default();
        let build_state = strategy
            .try_build(build_args)
            .expect_better("Expected a Better build outcome");

        let build = BuildState::from(build_state)
            .into_build(client, pool)
            .expect("Failed to convert build state into build");

        assert_eq!(payload_id, build.payload.id());
        assert_eq!(build.payload.block().body.len(), 4);
        assert_eq!(build.payload.block().body[0].hash(), bundle_tx_2.hash());
        assert_eq!(build.payload.block().body[1].hash(), bundle_tx_3.hash());
        assert_eq!(build.payload.block().body[2].hash(), bundle_tx_1.hash());
    }
    #[tokio::test]
    async fn bundle_inclusion_same_target() {
        let (build_config, client, _, pool, bundle_pool, payload_id) = setup_test_env();
        let build_args = BuildArguments {
            client,
            pool: pool.clone(),
            bundle_pool: bundle_pool.clone(),
            cached_reads: CachedReads::default(),
            build_config,
            cancel: Cancelled::default(),
            best_build: None,
        };

        let bundle_tx_1 = test_transaction(1, 0);
        let bundle_tx_2 = test_transaction(2, 0);
        let bundle_tx_3 = test_transaction(3, 0);
        add_test_bundle(
            &bundle_pool,
            test_bundle(1, vec![bundle_tx_1.clone(), bundle_tx_2.clone()]),
        );
        add_test_bundle(
            &bundle_pool,
            test_bundle(1, vec![bundle_tx_1.clone(), bundle_tx_3.clone()]),
        );

        let strategy = BundlesStrat::default();
        let build_state = strategy
            .try_build(build_args)
            .expect_better("Expected a Better build outcome");

        let build = BuildState::from(build_state)
            .into_build(client, pool)
            .expect("Failed to convert build state into build");

        assert_eq!(payload_id, build.payload.id());
        assert_eq!(build.payload.block().body.len(), 3);
        assert_eq!(build.payload.block().body[0].hash(), bundle_tx_1.hash());
        assert_eq!(build.payload.block().body[1].hash(), bundle_tx_3.hash());
    }

    #[tokio::test]
    async fn bundle_override_mempool() {
        let (build_config, client, _, pool, bundle_pool, payload_id) = setup_test_env();
        let build_args = BuildArguments {
            client,
            pool: pool.clone(),
            bundle_pool: bundle_pool.clone(),
            cached_reads: CachedReads::default(),
            build_config,
            cancel: Cancelled::default(),
            best_build: None,
        };

        let mempool_tx = test_transaction(1, 0);
        let bundle_tx = test_transaction(0, 0);
        add_tx_test_pool(&pool, mempool_tx.clone()).await;
        add_test_bundle(
            &bundle_pool,
            test_bundle(1, vec![mempool_tx.clone(), bundle_tx.clone()]),
        );

        let strategy = BundlesStrat::default();
        let build_state = strategy
            .try_build(build_args)
            .expect_better("Expected a Better build outcome");

        let build = BuildState::from(build_state)
            .into_build(client, pool)
            .expect("Failed to convert build state into build");

        assert_eq!(payload_id, build.payload.id());
        assert_eq!(build.payload.block().body.len(), 3);
        assert_eq!(build.payload.block().body[0].hash(), mempool_tx.hash());
        assert_eq!(build.payload.block().body[1].hash(), bundle_tx.hash());
    }

    #[tokio::test]
    async fn ofac() {
        let tx = ofac_tx();

        let (build_config, client, _, pool, bundle_pool, _) = setup_test_env();
        let build_args = BuildArguments {
            client,
            pool: pool.clone(),
            bundle_pool: bundle_pool.clone(),
            cached_reads: CachedReads::default(),
            build_config,
            cancel: Cancelled::default(),
            best_build: None,
        };

        add_tx_test_pool(&pool, tx.clone()).await;
        add_test_bundle(&bundle_pool, test_bundle(1, vec![tx.clone()]));

        let strategy = BundlesStrat::default();
        strategy.try_build(build_args).expect_aborted(
            "Expected the build to be aborted due to OFAC txs not included in pool",
        );
    }

    /// Test-only OFAC tx: signed with a fixed test key, "to" is a placeholder address (not a real contract).
    fn ofac_tx() -> TransactionSignedEcRecovered {
        let to = reth::primitives::Address::from_slice(&[0x02u8; 20]);
        let tx = Transaction::Legacy(TxLegacy {
            chain_id: Some(1),
            nonce: 2,
            gas_price: 10725159612,
            gas_limit: 21004,
            to: TransactionKind::Call(to),
            value: U256::from_str("65119306260154376").unwrap(),
            input: bytes!("00"),
        });
        let sk = SecretKey::from_str(
            "0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap();
        let sig = sign_message(B256::from_slice(sk.as_ref()), tx.signature_hash()).unwrap();
        TransactionSigned::from_transaction_and_signature(tx, sig)
            .into_ecrecovered()
            .unwrap()
    }
}
