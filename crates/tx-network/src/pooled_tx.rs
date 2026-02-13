use ethers::types::Transaction;

use reth::{primitives::Bytes, rpc::eth::error::EthApiError, transaction_pool::PoolTransaction};
use rpc::utils::recover_raw_transaction;
/// A trait representing a pooled transaction.
///
/// This trait provides functionality to convert a transaction into a pooled
/// transaction format, which is used within the transaction pool.
pub trait PooledTx {
    /// Converts a transaction into a pooled transaction.
    ///
    /// Implementations of this function should handle the conversion logic
    /// necessary to transform a general transaction into a format suitable
    /// for pooling.
    ///
    /// # Returns
    /// A result containing the pooled transaction or an error (`EthApiError`).
    fn into<PooledTx: PoolTransaction>(&self) -> Result<PooledTx, EthApiError>;
}

impl PooledTx for Transaction {
    /// Converts a `Transaction` into a pooled transaction.
    ///
    /// This implementation recovers the raw transaction and transforms it into
    /// a pooled transaction format.
    ///
    /// # Returns
    /// A result containing the pooled transaction or an error (`EthApiError`).
    fn into<PooledTx: PoolTransaction>(&self) -> Result<PooledTx, EthApiError> {
        let bytes = Bytes::from(self.rlp().0);
        let recovered = recover_raw_transaction(&bytes)?;
        Ok(PooledTx::from_recovered_pooled_transaction(recovered))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use ethers::types::{transaction::eip2930::AccessList, Address, Transaction, H256, U256};
    use reth::{
        primitives::hex,
        transaction_pool::{EthPooledTransaction, PoolTransaction},
    };

    use crate::pooled_tx::PooledTx;

    #[test]
    fn pooled_tx() {
        let tx = Transaction {
            hash: H256::from_str(
                "5e2fc091e15119c97722e9b63d5d32b043d077d834f377b91f80d32872c78109",
            )
            .unwrap(),
            nonce: 65.into(),
            block_hash: Some(
                H256::from_str("f43869e67c02c57d1f9a07bb897b54bec1cfa1feb704d91a2ee087566de5df2c")
                    .unwrap(),
            ),
            block_number: Some(6203173.into()),
            transaction_index: Some(10.into()),
            from: Address::from_str("e66b278fa9fbb181522f6916ec2f6d66ab846e04").unwrap(),
            to: Some(Address::from_str("11d7c2ab0d4aa26b7d8502f6a7ef6844908495c2").unwrap()),
            value: 0.into(),
            gas_price: Some(1500000007.into()),
            gas: 106703.into(),
            input: hex::decode("e5225381").unwrap().into(),
            v: 1.into(),
            r: U256::from_str_radix(
                "12010114865104992543118914714169554862963471200433926679648874237672573604889",
                10,
            )
            .unwrap(),
            s: U256::from_str_radix(
                "22830728216401371437656932733690354795366167672037272747970692473382669718804",
                10,
            )
            .unwrap(),
            transaction_type: Some(2.into()),
            access_list: Some(AccessList::default()),
            max_priority_fee_per_gas: Some(1500000000.into()),
            max_fee_per_gas: Some(1500000009.into()),
            chain_id: Some(5.into()),
            other: Default::default(),
        };
        let pooled_tx: EthPooledTransaction = PooledTx::into(&tx).unwrap();
        assert_eq!(pooled_tx.nonce(), 65);
    }
}
