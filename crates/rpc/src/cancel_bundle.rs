use async_trait::async_trait;
use bundles::pool::BundlePool;
use reth::primitives::B256;
use reth_rpc::eth::error::{EthApiError, EthResult};
use tracing::debug;

#[async_trait]
pub trait CancelBundle {
    async fn cancel(&self, pool: BundlePool) -> EthResult<()>;
}

#[async_trait]
impl CancelBundle for B256 {
    async fn cancel(&self, pool: BundlePool) -> EthResult<()> {
        let hash = *self;

        pool.cancel_bundle(hash)
            .ok_or(EthApiError::InternalEthError)?
            .map_err(Into::<EthApiError>::into)?;

        debug!(
            target: "rpc-ext",
            hash=hash.to_string(),
            "Cancelled bundle",
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{
        assert_validation_error, generate_random_key, sign_transaction, RpcTestSuite,
    };

    use builder_primitives::{rpc::EthSendBundle, U256};
    use bundles::{bundle::Bundle, pool::BundlePool};
    use reth::primitives::{AccessList, Bytes, Transaction, TransactionKind, TxEip1559, B256};
    use serde_json::json;

    #[ignore]
    #[tokio::test]
    async fn cancel_bundle_validation_errors() {
        let test_setup = RpcTestSuite::new("8558".to_string(), 1, 10, None, None).await;

        let response = test_setup
            .send_request("eth_cancelBundle".to_string(), json!([B256::random()]))
            .await;

        assert_validation_error("bundle not found".to_string(), response).await;
    }
    fn create_bundle(hash: B256) -> Bundle {
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
        let eth_bundle = &EthSendBundle {
            txs: vec![encoded],
            ..Default::default()
        };
        Bundle::from((eth_bundle, hash))
    }

    #[ignore]
    #[tokio::test]
    async fn cancel_bundle() {
        let hash = B256::random();
        let bundle_pool = BundlePool::new();
        let bundle = create_bundle(hash);
        bundle_pool.add_bundle(bundle.clone());
        assert_eq!(bundle_pool.len(), Some(1));

        let test_setup =
            RpcTestSuite::new("8555".to_string(), 1, 10, None, Some(bundle_pool)).await;

        let response = test_setup
            .send_request("eth_cancelBundle".to_string(), json!([hash]))
            .await;

        let status_code = response.status();
        assert_eq!(status_code, 200);

        assert!(test_setup.bundle_pool().is_empty().expect("lock acquired"));
    }
}
