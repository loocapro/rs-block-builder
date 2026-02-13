use crate::job::build_utils::Cancelled;
use crate::strategy::state::bundle::BundleValidator;
use crate::strategy::state::bundle::SimulatedBundle;
use crate::strategy::state::db::ExecutionDB;
use crate::strategy::state::BuildState;
use crate::strategy::state::BuildStateExecutionError;
use crate::utils::ofac_addresses::Ofac;
use builder_primitives::payload::PayloadConfig;
use bundles::bundle::Bundle;
use bundles::pool::BundlePool;
use reth::primitives::{IntoRecoveredTransaction, U64};
use reth::transaction_pool::BestTransactionsAttributes;
use reth::transaction_pool::TransactionPool;
use reth_payload_builder::error::PayloadBuilderError;
use revm_primitives::{EVMError, FixedBytes, InvalidTransaction};
use std::collections::HashSet;
use std::sync::Arc;

use tracing::{error, trace};

/// Error type for tx execution
pub enum TxExecution {
    /// Job was cancelled
    JobCancelled,
    /// One of the tx executions failed
    ExecFailed,
}

/// Bundle Strategy that builds blocks with current view of valid mev bundles and mempool transactions
/// Strategy first pulls all valid mev bundles from the bundle pool. Bundles are filtered by
/// successful recovery and top of block simulation before being ordered by bundle value.
/// Bundles are executed in this order only if the bundle does not contain a previously committed
/// transaction. Bundles with committed transactions are skipped due to lower sort priority.
/// Mempool transaction inclusion follows bundles.
#[derive(Debug, Clone, Default)]
pub struct BundlesStrat {
    /// The configuration for how the payload will be created.
    pub config: PayloadConfig,
}

impl BundlesStrat {
    /// Fetches and processes bundles from the bundle pool by a given block number and timestamp.
    /// It decodes valid bundles by recovering transactions within each bundle and filters out
    /// bundles with any simulation errors.
    ///
    /// # Parameters
    /// - `bundle_pool`: Reference to the bundle pool from which bundles are fetched.
    /// - `build_state`: Mutable reference to the current build state, used for simulation and bundle recovery.
    /// - `db`: Mutable reference to the execution database used for transaction recovery and execution.
    ///
    /// # Returns
    /// A `Result` containing either a vector of `SimulatedBundle` on success or a `PayloadBuilderError` on failure.
    pub fn decode_and_filter_bundles(
        &self,
        db: &mut ExecutionDB<'_>,
        build_state: &mut BuildState,
        bundle_pool: &BundlePool,
    ) -> Result<Vec<SimulatedBundle>, PayloadBuilderError> {
        let bundles = bundle_pool.bundles(
            U64::from(build_state.build_config().block_number),
            build_state.build_config().attributes.timestamp,
        ).unwrap_or_else(|| {
            error!(target: "payload::strategy::bundle", build_state = build_state.to_log(), "Failed to acquire bundle pool lock.");
            Vec::new()
        });

        let recovered_bundles = Bundle::recover_bundles(bundles);
        let simulations = recovered_bundles
            .into_iter()
            .filter_map(|bundle| build_state.simulate_bundle(db, bundle).ok())
            .collect();

        Ok(SimulatedBundle::sort_by_max_value(simulations))
    }

    /// Executes the provided bundles, applying a greedy selection algorithm to choose the best bundles
    /// based on profitability and avoiding execution of bundles with blacklisted transactions or already
    /// committed transactions.
    ///
    /// # Parameters
    /// - `processed_bundles`: Vector of `SimulatedBundle` that have been processed and are ready for execution.
    /// - `build_state`: Mutable reference to the current build state, used for bundle execution.
    /// - `db`: Mutable reference to the execution database for executing transactions within bundles.
    ///
    /// # Returns
    /// A `Result` containing either a set of committed transaction hashes on success or a `PayloadBuilderError` on failure.
    pub fn commit_selected_bundles(
        &self,
        db: &mut ExecutionDB<'_>,
        build_state: &mut BuildState,
        processed_bundles: Vec<SimulatedBundle>,
    ) -> Result<HashSet<FixedBytes<32>>, PayloadBuilderError> {
        // global set of all commited tx hashes
        let mut committed_txs = HashSet::new();

        // greedy selection of sorted bundles, skip bundles that fail real time execution
        for bundle in processed_bundles {
            if BundleValidator::contains_ofac(&bundle) {
                trace!(target: "payload::strategy::bundle", build_state = build_state.to_log(), "Skipping ofac blacklisted tx");
                continue;
            }
            // check if bundle has a transaction that was already committed
            // This implies that another bundle was prioritized higher containining the same
            // transaction
            if BundleValidator::contains_committed_transaction(&bundle, &committed_txs) {
                trace!(target: "payload::strategy::bundle", build_state = build_state.to_log(), "Skipping less valuable bundle with already committed transaction");
                continue;
            }

            let bundle_txs = bundle.txs();
            match build_state.execute_bundle(db, bundle.recovered_bundle()) {
                Ok(_) => {
                    // all bundle transactions were successfully committed
                    committed_txs.extend(bundle_txs);
                }
                Err(err) => match err {
                    BuildStateExecutionError::PayloadBuilder(err) => {
                        error!(target: "payload::strategy::bundle", build_state = build_state.to_log(), ?err, "Bundle execution failed");
                        return Err(err);
                    }
                    BuildStateExecutionError::BuilderRefundTx(err) => {
                        error!(target: "payload::strategy::bundle", build_state = build_state.to_log(), ?err, "Builder transfer failed");
                    }
                    _ => {
                        trace!(target: "payload::strategy::bundle", build_state = build_state.to_log(), ?err, "Skipping bundle after execution error");
                    }
                },
            }
        }
        Ok(committed_txs)
    }

    /// Processes transactions from the mempool, executing those not already included in processed bundles
    /// and not blacklisted by OFAC checks.
    ///
    /// # Type Parameters
    /// - `Pool`: A trait bound that specifies the type must implement the `TransactionPool` interface.
    ///
    /// # Parameters
    /// - `pool`: Reference to the transaction pool containing mempool transactions.
    /// - `build_state`: Mutable reference to the current build state, used for executing transactions.
    /// - `db`: Mutable reference to the execution database for executing transactions.
    /// - `committed_txs`: Mutable reference to the set of hashes of already committed transactions.
    /// - `cancel`: Reference to a cancellation token that allows early exit if the operation is cancelled.
    ///
    /// # Returns
    /// An outcome of the process as `ProcessOutcome`, indicating success, cancellation, or failure.
    pub fn execute_mempool_txs<Pool: TransactionPool>(
        &self,
        pool: &Pool,
        build_state: &mut BuildState,
        db: &mut ExecutionDB<'_>,
        committed_txs: &mut HashSet<FixedBytes<32>>,
        cancel: &Cancelled,
        best_txs_attrs: BestTransactionsAttributes,
    ) -> Result<(), TxExecution> {
        // collect all valid transactions in the mempool
        let mut best_txs = pool.best_transactions_with_attributes(best_txs_attrs);
        while let Some(pool_tx) = best_txs.next() {
            // check if the job was cancelled, if so we can exit early
            if cancel.is_cancelled() {
                return Err(TxExecution::JobCancelled);
            }

            let tx = pool_tx.to_recovered_transaction();

            // if signer is ofac blacklisted, skip transaction
            if Ofac::contains_ofac_addresses(&tx) {
                trace!(target: "payload::strategy::bundle", build_state = build_state.to_log(), ?pool_tx, "Skipping OFAC blacklisted transaction");
                best_txs.mark_invalid(&pool_tx);
                continue;
            }

            // check if bundle has a transaction that was already committed
            // This implies that the mempool transaction was already committed in a bundle
            if committed_txs.contains(&tx.hash()) {
                trace!(target: "payload::strategy::bundle", build_state = build_state.to_log(), "Skipping mempool transaction that was included in a bundle");
                continue;
            }

            // execute transaction on build state
            let tx_hash = tx.hash();
            match build_state.execute_transaction(db, tx) {
                Ok(_) => {
                    // transaction was successfully committed
                    committed_txs.insert(tx_hash);
                }
                Err(err) => {
                    trace!(target: "payload::strategy::bundle", build_state = build_state.to_log(), ?pool_tx, ?err, "Failed transaction execution");
                    match err {
                        BuildStateExecutionError::OfacAddressDetected(ofac_address) => {
                            error!(target: "payload::strategy::mempool", build_state = build_state.to_log(), ?ofac_address, "OFAC blacklisted transaction detected [from, to or internal calls]");
                            best_txs.mark_invalid(&pool_tx);
                        }
                        BuildStateExecutionError::MaxGasExceeded => {
                            best_txs.mark_invalid(&pool_tx);
                        }
                        BuildStateExecutionError::MaxBlobGasExceeded => {
                            best_txs.skip_blobs();
                            best_txs.mark_invalid(&pool_tx);
                        }
                        BuildStateExecutionError::PayloadBuilder(err) => match err {
                            PayloadBuilderError::EvmExecutionError(EVMError::Transaction(err)) => {
                                match err {
                                    InvalidTransaction::NonceTooLow { .. } => {}
                                    _ => {
                                        best_txs.mark_invalid(&pool_tx);
                                    }
                                }
                            }
                            _ => {
                                error!(target: "payload::strategy::bundle", build_state = build_state.to_log(), ?err, "Mempool transaction execution failed");
                                return Err(TxExecution::ExecFailed);
                            }
                        },
                        BuildStateExecutionError::BuilderRefundTx(_)
                        | BuildStateExecutionError::BundleExection(_)
                        | BuildStateExecutionError::BundleReverted(_) => {
                            unreachable!("Should not receive a bundle error");
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Helper enum to represent the outcome of the execution process
pub enum ProcessOutcome {
    /// The process was successful
    Ok,
    /// The process was successful and a better build state was found
    Better {
        /// The new build state
        build_state: Arc<BuildState>,
    },
    /// The process was aborted
    Aborted,
    /// The process was cancelled
    Cancelled,
}
