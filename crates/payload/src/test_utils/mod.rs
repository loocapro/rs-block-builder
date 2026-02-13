use reth::transaction_pool::{
    validate::ValidTransaction, EthPooledTransaction, PoolTransaction, TransactionOrigin,
    TransactionValidationOutcome, TransactionValidator,
};

/// Test utilities and mocks for the payload crate.
pub mod mock_provider;

/// Mock validator that accepts all transactions (for tests).
#[derive(Default, Debug)]
#[non_exhaustive]
pub struct MockValidator;

impl TransactionValidator for MockValidator {
    type Transaction = EthPooledTransaction;

    async fn validate_transaction(
        &self,
        _origin: TransactionOrigin,
        transaction: Self::Transaction,
    ) -> TransactionValidationOutcome<Self::Transaction> {
        // Always return valid
        TransactionValidationOutcome::Valid {
            balance: transaction.cost(),
            state_nonce: transaction.nonce(),
            transaction: ValidTransaction::Valid(transaction),
            propagate: false,
        }
    }
}
