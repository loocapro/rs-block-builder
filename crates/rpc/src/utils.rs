use reth::primitives::{
    Bytes, PooledTransactionsElement, PooledTransactionsElementEcRecovered, U64,
};
use reth_rpc::eth::error::{EthApiError, EthResult};

use crate::BundlesApiError;

/// Validates the bundle of transactions
/// - The bundle must not be empty
/// - The block number must be greater than 0
///
/// It returns a valid Vec of [PooledTransactionsElementEcRecovered] or an error
pub fn recover_transactions(
    txs: &[Bytes],
    block_number: U64,
) -> Result<Vec<PooledTransactionsElementEcRecovered>, EthApiError> {
    if txs.is_empty() {
        return Err(EthApiError::InvalidParams(
            BundlesApiError::EmptyBundleTransactions.to_string(),
        ));
    }
    if block_number.to::<u64>() == 0 {
        return Err(EthApiError::InvalidParams(
            BundlesApiError::BundleMissingBlockNumber.to_string(),
        ));
    }

    txs.iter()
        .map(&recover_raw_transaction)
        .collect::<Result<Vec<_>, _>>()
}

/// Recovers a [PooledTransactionsElementEcRecovered] from an enveloped encoded byte stream.
///
/// See [PooledTransactionsElement::decode_enveloped]
pub fn recover_raw_transaction(data: &Bytes) -> EthResult<PooledTransactionsElementEcRecovered> {
    if data.is_empty() {
        return Err(EthApiError::EmptyRawTransactionData);
    }

    let transaction = PooledTransactionsElement::decode_enveloped(&mut data.as_ref())
        .map_err(|_| EthApiError::FailedToDecodeSignedTransaction)?;

    transaction
        .try_into_ecrecovered()
        .or(Err(EthApiError::InvalidTransactionSignature))
}
