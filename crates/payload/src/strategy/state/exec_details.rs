use reth::primitives::{Receipt, TransactionSigned};
use revm::db::BundleState;
use revm_primitives::U256;

#[derive(Debug, Clone, Default)]
/// Summary of executed transactions
pub struct ExecutionDetails {
    /// Transactions executed on stack
    pub executed_txs: Vec<TransactionSigned>,
    /// Receipts of executed transactions
    pub receipts: Vec<Option<Receipt>>,
    /// Total gas used by executed transactions
    pub cumulative_gas_used: u64,
    /// Total blob gas used by executed transactions
    pub cumulative_blob_gas_used: u64,
    /// Total tip fees by executed transactions
    pub sum_fees: U256,
    /// Total coinbase transfers by executed transactions
    pub sum_coinbase_transfers: U256,
    /// Set payment by builder
    pub builder_payment: Option<U256>,
    /// EVM Bundle State
    pub bundle_state: Option<BundleState>,
}

impl ExecutionDetails {
    /// Sets the builder payment for this execution details.
    ///
    /// This method updates the `builder_payment` field with the specified payment amount.
    ///
    /// # Arguments
    ///
    /// * `payment` - The payment amount to set, encapsulated in a `U256` type to accommodate
    ///   large values typical in blockchain contexts.
    ///
    /// # Returns
    ///
    /// Returns a mutable reference to the `ExecutionDetails` instance to enable method chaining.
    pub fn set_builder_payment(&mut self, payment: U256) -> &Self {
        self.builder_payment = Some(payment);
        self
    }

    /// Sets the state of the bundle associated with this execution details.
    ///
    /// This method updates the `bundle_state` field with the specified bundle state.
    ///
    /// # Arguments
    ///
    /// * `bundle_state` - The state of the bundle to set, typically encapsulating various
    ///   statuses a transaction bundle might be in during its lifecycle.
    ///
    /// # Returns
    ///
    /// Returns a mutable reference to the `ExecutionDetails` instance, allowing for further
    /// modifications through method chaining.
    pub fn set_bundle_state(&mut self, bundle_state: BundleState) -> &Self {
        self.bundle_state = Some(bundle_state);
        self
    }
}
