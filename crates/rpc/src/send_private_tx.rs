use async_trait::async_trait;
use reth::{
    primitives::{Bytes, FromRecoveredPooledTransaction, B256},
    transaction_pool::TransactionOrigin,
};
use reth_rpc::eth::error::EthResult;
use tracing::debug;

use super::utils::recover_raw_transaction;
use reth::transaction_pool::TransactionPool;

#[async_trait]
pub trait PrivateTx {
    async fn send<Pool>(&self, pool: Pool) -> EthResult<B256>
    where
        Pool: TransactionPool + Clone + 'static;
}

#[async_trait]
impl PrivateTx for Bytes {
    async fn send<Pool>(&self, pool: Pool) -> EthResult<B256>
    where
        Pool: TransactionPool + Clone + 'static,
    {
        let recovered = recover_raw_transaction(self)?;
        let pool_transaction =
            <Pool::Transaction as FromRecoveredPooledTransaction>::from_recovered_pooled_transaction(
                recovered,
            );
        // submit the transaction to the pool as private
        let hash = pool
            .add_transaction(TransactionOrigin::Private, pool_transaction)
            .await?;
        debug!(
            target: "rpc-ext",
            hash=hash.to_string(),
            "New private transaction",
        );
        Ok(hash)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{generate_random_key, sign_transaction, RpcResponse, RpcTestSuite};

    use builder_primitives::U256;
    use reth::primitives::{AccessList, Bytes, Transaction, TransactionKind, TxEip1559, B256};
    use serde_json::json;

    #[ignore]
    #[tokio::test]
    async fn send_tx() {
        let (secret_key, address) = generate_random_key();

        let tx = sign_transaction(
            &secret_key,
            Transaction::Eip1559(TxEip1559 {
                chain_id: 1,
                nonce: 0,
                gas_limit: 21000,
                to: TransactionKind::Call(address),
                value: *U256::from(1_000_000).as_ref(),
                input: Bytes::default(),
                max_fee_per_gas: 0x4a817c800,
                max_priority_fee_per_gas: 0x3b9aca00,
                access_list: AccessList::default(),
            }),
        );
        let encoded = tx.envelope_encoded();

        let test_setup = RpcTestSuite::new("8552".to_string(), 1, 10, Some(address), None).await;

        let response = test_setup
            .send_request("eth_sendPrivateTransaction".to_string(), json!([encoded]))
            .await;

        let rpc_resp = response.json::<RpcResponse<B256>>().await.unwrap();
        assert_eq!(rpc_resp.result, tx.hash());

        assert_eq!(test_setup.pool().len(), 1);
    }
}
