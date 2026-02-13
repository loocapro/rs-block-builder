/// build types for job
pub mod build_utils;
/// interval based build job
pub mod interval_job;
/// metrics for job
pub mod metrics;
/// stream based build job
pub mod stream_job;

/// empty build job
pub mod empty;

#[cfg(test)]
pub(crate) mod tests {
    use builder_primitives::build::test_utils::build_test_config;
    use builder_primitives::payload::BuildConfig;
    use builder_primitives::U256;
    use bundles::bundle::Bundle;
    use bundles::pool::BundlePool;
    use futures_core::Future;
    use futures_util::FutureExt;
    use reth::primitives::constants::GWEI_TO_WEI;
    use reth::{
        primitives::{AccessList, TransactionSignedEcRecovered, U64},
        primitives::{Transaction, TransactionKind, TxEip1559},
        tasks::TaskManager,
        transaction_pool::{
            blobstore::InMemoryBlobStore, CoinbaseTipOrdering, EthPooledTransaction, Pool,
            TransactionOrigin,
        },
    };
    use reth_payload_builder::PayloadId;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };
    use tokio::runtime::Handle;

    use reth::primitives::{sign_message, Address, Bytes, TransactionSigned, B256};
    use reth::transaction_pool::TransactionPool;

    use crate::test_utils::MockValidator;
    use crate::{test_utils::mock_provider::MockProvider, traits::PayloadJob};
    use reth_payload_builder::error::PayloadBuilderError;

    #[derive(Debug)]
    pub(crate) struct TestBuildJob<Job: PayloadJob> {
        pub(crate) inner: Job,
    }

    impl<Job> Future for TestBuildJob<Job>
    where
        Job: PayloadJob + Unpin + 'static,
    {
        type Output = Result<Job::ResolvePayloadFuture, PayloadBuilderError>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.get_mut();
            match this.inner.poll_unpin(cx) {
                Poll::Ready(Ok(_)) => {
                    let (fut, _) = this.inner.resolve();
                    Poll::Ready(Ok(fut))
                }
                Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    pub(crate) type TestPool =
        Pool<MockValidator, CoinbaseTipOrdering<EthPooledTransaction>, InMemoryBlobStore>;

    pub(crate) fn setup_test_env() -> (
        BuildConfig,
        MockProvider,
        TaskManager,
        TestPool,
        BundlePool,
        PayloadId,
    ) {
        let payload_id = PayloadId::new([0; 8]);
        let config = build_test_config(payload_id);
        let client = MockProvider::default();
        let task_manager = TaskManager::new(Handle::current());
        let pool = Pool::new(
            MockValidator::default(),
            CoinbaseTipOrdering::default(),
            InMemoryBlobStore::default(),
            Default::default(),
        );
        let bundle_pool = BundlePool::default();
        (config, client, task_manager, pool, bundle_pool, payload_id)
    }

    pub(crate) fn test_transaction(
        prio_gwei: u64,
        coinbase_gwei: u64,
    ) -> TransactionSignedEcRecovered {
        let secret = B256::random();
        let tx = Transaction::Eip1559(TxEip1559 {
            chain_id: 1,
            nonce: 0,
            gas_limit: 21000,
            max_fee_per_gas: (prio_gwei * GWEI_TO_WEI).into(),
            max_priority_fee_per_gas: (prio_gwei * GWEI_TO_WEI).into(),
            to: TransactionKind::Call(Address::ZERO),
            value: *U256::from(coinbase_gwei * 21000 * GWEI_TO_WEI).as_ref(),
            access_list: AccessList::default(),
            input: Bytes::default(),
        });
        let hash = tx.signature_hash();
        let sig = sign_message(secret, hash).expect("sign fail");
        let tx_signed = TransactionSigned::from_transaction_and_signature(tx, sig);
        tx_signed.into_ecrecovered().expect("recover fail")
    }

    pub(crate) fn test_bundle(block_number: u64, txs: Vec<TransactionSignedEcRecovered>) -> Bundle {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Bundle {
            hash: B256::random(),
            txs: txs.into_iter().map(|tx| tx.envelope_encoded()).collect(),
            min_timestamp: Some(timestamp),
            max_timestamp: Some(timestamp),
            block_number: U64::from(block_number),
            ..Default::default()
        }
    }

    pub(crate) async fn add_tx_test_pool(pool: &TestPool, tx: TransactionSignedEcRecovered) {
        let start_pool_size = pool.pool_size().pending;
        let result = pool
            .add_transaction(
                TransactionOrigin::External,
                EthPooledTransaction::new(tx.clone(), tx.length_without_header()),
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(pool.pool_size().pending, start_pool_size + 1);
    }

    pub(crate) fn add_test_bundle(bundle_pool: &BundlePool, bundle: Bundle) {
        let start_pool_size = bundle_pool.len().expect("lock acquired");
        bundle_pool.add_bundle(bundle);
        assert_eq!(bundle_pool.len(), Some(start_pool_size + 1));
    }
}
