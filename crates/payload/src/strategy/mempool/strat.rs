use crate::job::build_utils::Cancelled;
use crate::strategy::bundle::strat::TxExecution;
use crate::strategy::state::db::ExecutionDB;
use crate::strategy::state::BuildState;
use crate::strategy::state::BuildStateExecutionError;
use crate::utils::ofac_addresses::Ofac;
use builder_primitives::payload::PayloadConfig;
use reth::primitives::IntoRecoveredTransaction;
use reth::transaction_pool::BestTransactionsAttributes;
use reth::transaction_pool::TransactionPool;
use reth_payload_builder::error::PayloadBuilderError;
use revm_primitives::EVMError;
use revm_primitives::InvalidTransaction;
use tracing::{error, trace};

/// Mempool Strategy that builds blocks with current view of valid mempool transactions
#[derive(Debug, Clone, Default)]
pub struct MempoolStrat {
    /// The configuration for how the payload will be created.
    pub config: PayloadConfig,
}
impl MempoolStrat {
    /// Processes transactions from the mempool, executing those not blacklisted by OFAC checks.
    ///
    /// # Type Parameters
    /// - `Pool`: A trait bound that specifies the type must implement the `TransactionPool` interface.
    ///
    /// # Parameters
    /// - `pool`: Reference to the transaction pool containing mempool transactions.
    /// - `build_state`: Mutable reference to the current build state, used for executing transactions.
    /// - `db`: Mutable reference to the execution database for executing transactions.
    /// - `cancel`: Reference to a cancellation token that allows early exit if the operation is cancelled.
    ///
    /// # Returns
    /// An outcome of the process as `ProcessOutcome`, indicating success, cancellation, or failure.
    pub fn execute_txs<Pool: TransactionPool>(
        &self,
        pool: &Pool,
        build_state: &mut BuildState,
        db: &mut ExecutionDB<'_>,
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

            if let Err(err) = build_state.execute_transaction(db, tx) {
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

        Ok(())
    }
}
