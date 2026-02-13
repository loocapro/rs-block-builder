use async_trait::async_trait;

use crate::utils::recover_transactions;
use builder_primitives::rpc::EthSendBundle;
use bundles::{bundle::Bundle, pool::BundlePool};
use reth::rpc::types::EthBundleHash;
use reth_rpc::eth::error::{EthApiError, EthResult};
use tracing::debug;
use uuid::Uuid;

#[async_trait]
pub trait SendBundle {
    async fn send(&self, bundle_pool: BundlePool) -> EthResult<EthBundleHash>;
}

#[async_trait]
impl SendBundle for EthSendBundle {
    /// Sends a bundle of transactions. The sender is responsible for signing the
    /// transactions and using the correct nonce and ensuring validity
    async fn send(&self, bundle_pool: BundlePool) -> EthResult<EthBundleHash> {
        let req_id = Uuid::new_v4();

        debug!(
            target: "rpc-ext",
            id=req_id.to_string(),
            bundle=format!("{:?}", self),
            "New bundle",
        );
        let metrics = bundle_pool.metrics();

        let txs = recover_transactions(&self.txs, self.block_number)?;
        let tx_hashes: Vec<_> = txs.iter().map(|tx| tx.hash()).collect();

        let bundle_hash = Bundle::create_hash(&txs);

        match bundle_pool.add_bundle(Bundle::from((self, bundle_hash))) {
            Some(bundle_hash) => {
                debug!(
                    target: "rpc-ext",
                    id=req_id.to_string(),
                    hash=bundle_hash.to_string(),
                    txs=format!("{:?}", tx_hashes),
                    bundle=format!("{:?}", self),
                    "Added bundle",
                );
                metrics.inc_send_bundle_rpc_success();
                Ok(EthBundleHash { bundle_hash })
            }
            None => {
                tracing::error!(
                    target: "rpc-ext",
                    id=req_id.to_string(),
                    bundle=format!("{:?}", self),
                    "Failed to add bundle, could not acquire lock. ",
                );
                metrics.inc_send_bundle_rpc_failure();
                Err(EthApiError::InternalEthError)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::test_utils::{
        assert_validation_error, generate_random_key, sign_transaction, RpcResponse, RpcTestSuite,
    };
    use builder_primitives::U256;
    use reqwest::Response;

    use futures::future::join_all;
    use reth::{
        primitives::{
            keccak256, AccessList, Address, Bytes, BytesMut, Transaction, TransactionKind,
            TransactionSigned, TxEip1559, U64,
        },
        rpc::types::EthBundleHash,
    };
    use secp256k1::SecretKey;
    use serde_json::json;

    #[ignore]
    #[tokio::test]
    async fn sendbundle_validation_errors() {
        let test_setup = RpcTestSuite::new("8551".to_string(), 1, 10, None, None).await;

        let response = test_setup
            .send_request(
                "eth_sendBundle".to_string(),
                json!([{
                    "txs": [],
                    "blockNumber": 0,
                    "stateBlockNumber": "0x1"
                }]),
            )
            .await;

        assert_validation_error("bundle missing txs".to_string(), response).await;

        let test_setup = RpcTestSuite::new("8550".to_string(), 1, 10, None, None).await;

        let response = test_setup
            .send_request(
                "eth_sendBundle".to_string(),
                json!([{
                    "txs": ["0x","0x"],
                    "blockNumber": 0,
                    "stateBlockNumber": "0x1"
                }]),
            )
            .await;

        assert_validation_error("bundle missing blockNumber".to_string(), response).await;
    }

    #[ignore]
    #[tokio::test]
    async fn send_bundle() {
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
        let mut encoded = BytesMut::default();
        TransactionSigned::encode_enveloped(&tx, &mut encoded);

        let test_setup = RpcTestSuite::new("8549".to_string(), 1, 10, Some(address), None).await;

        let response = test_setup
            .send_request(
                "eth_sendBundle".to_string(),
                json!([{
                    "txs": [encoded],
                    "blockNumber": 1,
                    "stateBlockNumber": "0x1",
                    "refundPercent": 90,
                    "refundIndex": 0,
                    "refundRecipient": Address::default().to_string(),
                }]),
            )
            .await;

        let rpc_resp = response.json::<RpcResponse<EthBundleHash>>().await.unwrap();
        let send_bundle_resp = rpc_resp.result;

        assert_eq!(
            send_bundle_resp.bundle_hash,
            keccak256(tx.hash().as_slice())
        );

        let max_ts = Duration::from_secs(10000).as_secs();
        let bundles = test_setup
            .bundle_pool()
            .bundles(U64::from(1), max_ts)
            .expect("lock acquired");
        assert_eq!(bundles.len(), 1);
        let target = bundles[0].as_ref().clone();
        assert!(target.refund.is_some())
    }

    #[ignore]
    #[tokio::test]
    async fn send_many_bundles() {
        let test_setup = RpcTestSuite::new("8560".to_string(), 1, 10, None, None).await;

        let number_of_bundles = 100;

        let mut tasks = Vec::new();
        for _ in 0..number_of_bundles {
            let (secret_key, _address) = generate_random_key();
            let tx = create_tx(secret_key);
            let test_setup = test_setup.clone();

            let task = tokio::spawn(async move {
                let _resp = test_setup
                    .send_request(
                        "eth_sendBundle".to_string(),
                        json!([{
                            "txs": [tx],
                            "blockNumber": 1,
                            "stateBlockNumber": "0x1",
                        }]),
                    )
                    .await;
            });
            tasks.push(task);
        }

        join_all(tasks).await;
        assert_eq!(test_setup.bundle_pool().len(), Some(100));
    }

    async fn send_local_request(
        rpc_method: String,
        params: serde_json::Value,
    ) -> eyre::Result<Response> {
        let url = "http://localhost:8545";
        let client = reqwest::Client::builder().build().unwrap();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());

        match client
            .post(url)
            .headers(headers)
            .json(&json!({
                "jsonrpc": "2.0",
                "method": rpc_method,
                "params": params,
                "id": 1
            }))
            .send()
            .await
        {
            Ok(resp) => Ok(resp),
            Err(e) => {
                println!("Error sending request: {:?}", e);
                Err(e.into())
            }
        }
    }

    fn create_tx(secret_key: SecretKey) -> BytesMut {
        let tx = sign_transaction(
            &secret_key,
            Transaction::Eip1559(TxEip1559 {
                chain_id: 1,
                nonce: 0,
                gas_limit: 21000,
                to: TransactionKind::Call(Address::random()),
                value: *U256::from(1_000_000).as_ref(),
                input: Bytes::default(),
                max_fee_per_gas: 0x4a817c800,
                max_priority_fee_per_gas: 0x3b9aca00,
                access_list: AccessList::default(),
            }),
        );
        let mut encoded = BytesMut::default();
        TransactionSigned::encode_enveloped(&tx, &mut encoded);
        encoded
    }

    #[ignore]
    #[tokio::test]
    async fn send_many_bundles_local() {
        let (secret_key, _address) = generate_random_key();

        let number_of_bundles = 800;

        let mut tasks = Vec::new();
        for _ in 0..number_of_bundles {
            let tx = create_tx(secret_key);

            let task = tokio::spawn(async move {
                let resp = send_local_request(
                    "eth_sendBundle".to_string(),
                    json!([{
                        "txs": [tx],
                        "blockNumber": 1,
                        "stateBlockNumber": "0x1",
                    }]),
                )
                .await;
                if let Ok(resp) = resp {
                    let text = resp.text().await.unwrap();
                    println!("resp: {:?}", text)
                }
            });
            tasks.push(task);
        }

        join_all(tasks).await;
    }
}
