use std::sync::Arc;
use std::time::Instant;

use reth::providers::StateProviderFactory;
use reth::revm::{
    database::StateProviderDatabase, db::states::bundle_state::BundleRetention, State,
};
use reth::transaction_pool::{BestTransactionsAttributes, TransactionPool};
use revm_primitives::U256;

use crate::job::metrics::PayloadBuilderMetrics;
use crate::strategy::bundle::strat::TxExecution;
use crate::strategy::{state::BuildState, BuildArguments, BuildOutcome, BuildStrategy};
use reth_payload_builder::error::PayloadBuilderError;
use tracing::trace;

/// Mempool strategy implementation
pub mod strat;
pub use strat::MempoolStrat;

impl BuildStrategy for MempoolStrat {
    fn try_build<Client: StateProviderFactory + Clone, Pool: TransactionPool + Clone>(
        &self,
        args: BuildArguments<Client, Pool>,
    ) -> Result<BuildOutcome, PayloadBuilderError> {
        let BuildArguments {
            client,
            pool,
            mut cached_reads,
            build_config,
            cancel,
            best_build,
            ..
        } = args;

        let build_start = Instant::now();

        let state_provider = client.state_by_block_hash(build_config.parent_block.hash())?;
        let state = StateProviderDatabase::new(state_provider);
        let mut db = State::builder()
            .with_database_ref(cached_reads.as_db(&state))
            .with_bundle_update()
            .build();

        let base_fee = build_config.initialized_block_env.basefee.to::<u64>();
        let blob_gas_price = build_config
            .initialized_block_env
            .get_blob_gasprice()
            .map(|gasprice| gasprice as u64);

        let mut build_state = BuildState::new(build_config, self.config.clone(), Instant::now());
        trace!(target: "payload::strategy::empty", build_state = build_state.to_log(), "Building new build state");

        // apply eip-4788 pre block contract call
        build_state.pre_block_beacon_root_contract_call(&mut db)?;

        let best_tx_attributes = BestTransactionsAttributes::new(base_fee, blob_gas_price);

        match self.execute_txs(
            &pool,
            &mut build_state,
            &mut db,
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
    use super::*;
    use crate::job::build_utils::Cancelled;
    use crate::job::tests::{add_tx_test_pool, setup_test_env, test_transaction};
    use crate::strategy::BuildOutcome;
    use bundles::pool::BundlePool;
    use reth::primitives::constants::GWEI_TO_WEI;
    use reth::primitives::U256;
    use reth::primitives::{
        sign_message, Transaction, TransactionKind, TransactionSigned,
        TransactionSignedEcRecovered, TxLegacy, B256,
    };
    use reth_payload_builder::database::CachedReads;
    use revm_primitives::bytes;
    use secp256k1::SecretKey;
    use std::str::FromStr;

    #[tokio::test]
    async fn single_tx() {
        let (build_config, client, _, pool, _, _payload_id) = setup_test_env();

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

        let client = build_args.client;
        let pool = build_args.pool.clone();

        assert_try_build_outcome(
            client,
            pool,
            MempoolStrat::default().try_build(build_args.clone()),
            BuildOutcomeExpectation::Better {
                expected_tx_count: 2,
            },
        );
    }

    #[tokio::test]
    async fn ordering() {
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

        let client = build_args.client;
        let pool = build_args.pool.clone();
        let strategy = MempoolStrat::default();
        if let Ok(BuildOutcome::Better { build_state, .. }) = strategy.try_build(build_args) {
            let build = BuildState::from(build_state)
                .into_build(client, pool)
                .expect("failed build");

            assert_eq!(payload_id, build.payload.id());
            assert_eq!(build.payload.block().body.len(), 4);

            let expected_fees = [
                (3 * GWEI_TO_WEI).into(),
                (2 * GWEI_TO_WEI).into(),
                GWEI_TO_WEI.into(),
            ];
            for (index, &expected_fee) in expected_fees.iter().enumerate() {
                assert_eq!(
                    build.payload.block().body[index]
                        .transaction
                        .max_priority_fee_per_gas(),
                    Some(expected_fee),
                    "Transaction at index {} has incorrect max_priority_fee_per_gas",
                    index
                );
            }
        } else {
            unreachable!("Expected a better build outcome");
        }
    }

    #[tokio::test]
    async fn handle_ofac_tx() {
        let tx = ofac_tx();

        let (build_config, client, _, pool, _, _) = setup_test_env();

        let build_args = BuildArguments {
            client,
            pool: pool.clone(),
            bundle_pool: BundlePool::default(),
            cached_reads: CachedReads::default(),
            build_config,
            cancel: Cancelled::default(),
            best_build: None,
        };

        add_tx_test_pool(&pool, tx).await;

        assert_try_build_outcome(
            client,
            pool,
            MempoolStrat::default().try_build(build_args.clone()),
            BuildOutcomeExpectation::Aborted,
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
    fn assert_try_build_outcome<Client: StateProviderFactory, Pool: TransactionPool>(
        client: Client,
        pool: Pool,
        result: Result<BuildOutcome, PayloadBuilderError>,
        expectation: BuildOutcomeExpectation,
    ) {
        match (result, expectation) {
            (Ok(BuildOutcome::Aborted { .. }), BuildOutcomeExpectation::Aborted) => {}
            (
                Ok(BuildOutcome::Better { build_state, .. }),
                BuildOutcomeExpectation::Better { expected_tx_count },
            ) => {
                let build = BuildState::from(build_state)
                    .into_build(client, pool)
                    .expect("failed build");
                assert_eq!(build.payload.block().body.len(), expected_tx_count);
            }
            _ => unreachable!(),
        }
    }

    enum BuildOutcomeExpectation {
        Aborted,
        Better { expected_tx_count: usize },
    }
}
