use async_trait::async_trait;
use builder_primitives::rpc::EthSendBundle;
use bundles::pool::BundlePool;
use cancel_bundle::CancelBundle;
use jsonrpsee::proc_macros::rpc;
use reth::{
    primitives::{Bytes, B256},
    rpc::{eth::error::EthResult, types::EthBundleHash},
    tasks::pool::BlockingTaskGuard,
    transaction_pool::TransactionPool,
};
use reth_rpc::eth::EthTransactions;

use send_bundle::SendBundle;
use send_private_tx::PrivateTx;
use thiserror::Error;
use tokio::sync::{AcquireError, OwnedSemaphorePermit};

mod cancel_bundle;
mod send_bundle;
mod send_private_tx;
#[cfg(test)]
pub mod test_utils;
pub mod utils;
/// Eth bundle rpc interface.
///
/// See also <https://docs.flashbots.net/flashbots-auction/searchers/advanced/rpc-endpoint>

/// Eth bundle rpc interface.
///
/// See also <https://docs.flashbots.net/flashbots-auction/searchers/advanced/rpc-endpoint>
#[rpc(server, namespace = "eth")]
#[async_trait::async_trait]
pub trait BundlesApi {
    /// `eth_sendBundle` can be used to send bundles to the bundle pool.
    #[method(name = "sendBundle")]
    async fn send_bundle(&self, bundle: EthSendBundle) -> EthResult<EthBundleHash>;

    /// `eth_cancelBundle` is used to prevent a submitted bundle from being included on-chain. See [bundle cancellations](https://docs.flashbots.net/flashbots-auction/searchers/advanced/bundle-cancellations) for more information.
    #[method(name = "cancelBundle")]
    async fn cancel_bundle(&self, hash: B256) -> EthResult<()>;

    /// `eth_sendPrivateTransaction` is used to send a private transaction to the tx pool.
    /// This transaction will not be propagated via p2p and will be used to payload building.
    #[method(name = "sendPrivateTransaction")]
    async fn send_private_tx(&self, tx: Bytes) -> EthResult<B256>;
}

/// The type that implements `BundlesApi` rpc namespace trait
pub struct BundlesApiExt<Provider, EthApi, Pool> {
    pub provider: Provider,
    pub eth_api: EthApi,
    pub pool: Pool,
    pub bundles_pool: BundlePool,
    pub blocking_pool_guard: BlockingTaskGuard,
}

impl<Provider, EthApi, Pool> BundlesApiExt<Provider, EthApi, Pool> {
    /// Acquires a permit to execute a call.
    async fn acquire_trace_permit(&self) -> Result<OwnedSemaphorePermit, AcquireError> {
        self.blocking_pool_guard.clone().acquire_owned().await
    }
}

#[async_trait]
impl<Provider, EthApi, Pool> BundlesApiServer for BundlesApiExt<Provider, EthApi, Pool>
where
    Provider: 'static + Sync + Send + Clone,
    EthApi: EthTransactions + 'static + Sync + Send + Clone,
    Pool: TransactionPool + Clone + 'static,
{
    async fn send_bundle(&self, bundle: EthSendBundle) -> EthResult<EthBundleHash> {
        let _permit = self.acquire_trace_permit().await;

        bundle.send(self.bundles_pool.clone()).await
    }
    async fn send_private_tx(&self, tx: Bytes) -> EthResult<B256> {
        let _permit = self.acquire_trace_permit().await;
        tx.send(self.pool.clone()).await
    }
    async fn cancel_bundle(&self, hash: B256) -> EthResult<()> {
        let _permit = self.acquire_trace_permit().await;
        hash.cancel(self.bundles_pool.clone()).await
    }
}

/// [BundlesApi] specific errors.
#[derive(Debug, Error)]
pub enum BundlesApiError {
    /// Thrown if the bundle does not contain any transactions.
    #[error("bundle missing txs")]
    EmptyBundleTransactions,
    /// Thrown if the bundle does not contain a block number, or block number is 0.
    #[error("bundle missing blockNumber")]
    BundleMissingBlockNumber,
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{RpcErrResponse, RpcTestSuite};

    use serde_json::json;

    #[ignore]
    #[tokio::test]
    async fn eth_call_should_not_succeed() {
        let test_setup = RpcTestSuite::new("8545".to_string(), 1, 10, None, None).await;

        let response = test_setup
            .send_request("eth_call".to_string(), json!([{}]))
            .await;

        let to_json = response.json::<RpcErrResponse>().await.unwrap();
        assert_eq!(to_json.error.message, "Method not found");
    }
}
