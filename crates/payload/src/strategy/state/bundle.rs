use bundles::bundle::RecoveredBundle;
use reth::primitives::{TransactionSignedEcRecovered, B256, U256};
use std::{cmp::Ordering, collections::HashSet};

use crate::utils::ofac_addresses::Ofac;

use super::transaction::{SimulatedTransaction, TransactionSimulationOutcome};

/// Simulated bundle that encapsulates original recovered bundle, simulad transactions and final
/// bundle outcome
#[derive(Debug, Clone, Default)]
pub struct SimulatedBundle {
    /// Recovered bundle
    pub bundle: RecoveredBundle,
    /// Tx simulation outcomes
    simulated_txs: Vec<SimulatedTransaction>,
    /// Bundle simulation outcome
    outcome: BundleSimulationOutcome,
}

impl SimulatedBundle {
    /// Create new SimulatedBundle struct
    pub fn new(
        bundle: RecoveredBundle,
        simulated_txs: Vec<SimulatedTransaction>,
        base_fee: u64,
    ) -> Self {
        let outcome = BundleSimulationOutcome::from_txs(&bundle, &simulated_txs);
        let mut simulated = Self {
            bundle,
            simulated_txs,
            outcome,
        };

        // calculate and set refund amount
        let refund = simulated.calculate_refund(base_fee);
        simulated.outcome.refund = refund;
        simulated
    }

    /// Process top of block simulations
    /// Responsible for sorting bundles for inclusion priority
    pub fn sort_by_max_value(mut bundles: Vec<Self>) -> Vec<Self> {
        // sort bundles based on some criteria defined within BundleSimulationOutcome
        bundles.sort_by(|a, b| a.outcome.bundle_sort_by_max_value(&b.outcome));
        bundles
    }

    /// Calculates the refund for this bundle at given base fee.
    ///
    /// This function calculates the refund based on the provided `base_fee`. If the bundle
    /// is a refund bundle, it retrieves the refund index and the simulated
    /// transaction.
    ///
    /// # Arguments
    ///
    /// * `base_fee` - The base fee used for calculating the refund.
    ///
    /// # Returns
    ///
    /// Returns the calculated refund amount as a `U256`.
    pub fn calculate_refund(&self, base_fee: u64) -> U256 {
        let bundle_refund = match &self.bundle.refund {
            Some(refund) => refund,
            None => return U256::ZERO,
        };

        self.bundle
            .refund_index()
            .and_then(|index| self.simulated_txs.get(index))
            .map(|refund_tx| {
                bundle_refund.refund_amount(
                    refund_tx.outcome().fees(),
                    refund_tx.outcome().coinbase_transfer(),
                    base_fee,
                )
            })
            .unwrap_or(U256::ZERO)
    }

    /// Get original recovered bundle
    pub fn recovered_bundle(&self) -> RecoveredBundle {
        self.bundle.clone()
    }

    /// Get all tx hashes in bundle
    pub fn txs(&self) -> Vec<B256> {
        self.bundle.txs()
    }

    /// Get all recovered txs in bundle
    pub fn recovered_txs(&self) -> Vec<TransactionSignedEcRecovered> {
        self.bundle.recovered_txs()
    }

    /// Get all recovered txs in bundle
    pub fn recovered_txs_as_ref(&self) -> &Vec<TransactionSignedEcRecovered> {
        self.bundle.recovered_txs_as_ref()
    }

    /// Get all full simulated txs
    pub fn simulated_txs(&self) -> Vec<SimulatedTransaction> {
        self.simulated_txs.clone()
    }

    /// Get bundle outcome
    pub fn outcome(&self) -> &BundleSimulationOutcome {
        &self.outcome
    }

    /// Add a transaction to simulated bundle and update outcome fields
    pub fn add_tx(&mut self, tx: SimulatedTransaction) {
        self.outcome.add_tx_outcome(tx.outcome());
        self.simulated_txs.push(tx);
    }
}

/// Helper trait for validating bundles.
/// This trait is implemented for Vec<TransactionSignedEcRecovered> and is used to check if a bundle
/// contains a committed transaction or if it contains an OFAC blacklisted address.
pub trait BundleValidator {
    /// Checks if any of the transactions in `txs` are contained in the `committed` set.
    ///
    /// # Arguments
    /// * `committed` - A reference to a HashSet of transaction hashes representing committed transactions.
    /// * `txs` - An iterable of transactions to check against the committed set.
    ///
    /// # Returns
    /// `true` if at least one transaction from `txs` is in `committed`, otherwise `false`.
    fn contains_committed_transaction(&self, committed: &HashSet<B256>) -> bool;

    /// Checks if any of the transactions in `txs` are contained in the OFAC blacklist.
    fn contains_ofac(&self) -> bool;
}

impl BundleValidator for SimulatedBundle {
    fn contains_committed_transaction(&self, committed: &HashSet<B256>) -> bool {
        self.bundle
            .recovered_txs_as_ref()
            .iter()
            .any(|tx| committed.contains(&tx.hash()))
    }

    fn contains_ofac(&self) -> bool {
        self.bundle
            .recovered_txs_as_ref()
            .iter()
            .any(Ofac::contains_ofac_addresses)
    }
}

/// Outcome of bundle simulation
/// Built from individual transaction simulations
#[derive(Debug, Clone, Default)]
pub struct BundleSimulationOutcome {
    /// Whether a transaction reverted, Some(1) indicates tx at index 1 reverted
    pub reverted: Option<usize>,
    /// Gas used during simulation
    pub gas_used: u64,
    /// Blob gas used during simulation
    pub blob_gas_used: u64,
    /// Priority fees calculated
    pub fees: U256,
    /// Coinbase transfers detected
    pub coinbase_transfer: U256,
    /// Refund amount for this bundle
    pub refund: U256,
}

impl BundleSimulationOutcome {
    /// Generate a [`BundleSimulationOutcome`] from bundle transaction simulation outcomes
    /// Constructs a `BundleSimulationOutcome` from a slice of `TransactionSimulationOutcome`.
    ///
    /// This function aggregates outcomes of simulations to determine the overall
    /// result of a bundle of transactions. It calculates the total gas used,
    /// the total blob gas used, total fees, and the total coinbase transfer.
    /// Additionally, it identifies if and where the bundle of transactions
    /// reverted.
    ///
    /// # Arguments
    /// * `outcomes` - A slice of `TransactionSimulationOutcome` objects, each representing
    ///   the outcome of a transaction simulation.
    ///
    /// # Returns
    /// A `BundleSimulationOutcome` representing the aggregate results of the transaction
    /// simulations. This includes the index of the first transaction that
    /// reverted (if any), and the aggregated gas metrics and fees.
    ///
    /// If all transactions are successful, `reverted` is set to `None`.
    /// Otherwise, it's set to the 0-based index of the first failed transaction.
    pub fn from_txs(
        bundle: &RecoveredBundle,
        simulated_txs: &[SimulatedTransaction],
    ) -> BundleSimulationOutcome {
        let mut reverted = None;
        let mut gas_used = 0u64;
        let mut blob_gas_used = 0u64;
        let mut fees = U256::ZERO;
        let mut coinbase_transfer = U256::ZERO;

        for (index, simulated) in simulated_txs.iter().enumerate() {
            // check that simulation did not revert
            // If reverted, check allowed reverting_tx_hashes if tx is included, allow revert in
            // bundle
            if !simulated.outcome().evm_result.is_success()
                & !bundle.reverting_tx_hashes.contains(&simulated.tx().hash())
            {
                reverted = Some(index);
                break;
            }
            gas_used += simulated.outcome().gas_used();
            blob_gas_used += simulated.outcome().blob_gas_used();
            fees += simulated.outcome().fees();
            coinbase_transfer += simulated.outcome().coinbase_transfer();
        }

        BundleSimulationOutcome {
            reverted,
            gas_used,
            blob_gas_used,
            fees,
            coinbase_transfer,
            refund: U256::ZERO,
        }
    }

    /// Get bundle total value
    pub fn total_value(&self) -> U256 {
        self.fees
            .saturating_add(self.coinbase_transfer)
            .saturating_sub(self.refund)
    }

    /// Get bundle refund value
    pub fn refund(&self) -> U256 {
        self.refund
    }

    /// Sort bundles by max value
    pub fn bundle_sort_by_max_value(&self, other: &BundleSimulationOutcome) -> Ordering {
        (other.total_value()).cmp(&self.total_value())
    }

    /// Update bundle outcome with transaction outcome
    pub fn add_tx_outcome(&mut self, tx: &TransactionSimulationOutcome) {
        self.gas_used += tx.gas_used;
        self.blob_gas_used += tx.blob_gas_used;
        self.fees += tx.fees;
        self.coinbase_transfer += tx.coinbase_transfer;
    }
}
