use crate::BundlesApiServer;
use bundles::pool::BundlePool;
use rand::Rng;
use reqwest::Response;
use reth::{
    network::noop::NoopNetwork,
    primitives::{
        constants::ETHEREUM_BLOCK_GAS_LIMIT, keccak256, sign_message, Address, Block, Header,
        Transaction, TransactionSigned, B256, U256,
    },
    providers::{
        test_utils::{ExtendedAccount, MockEthProvider, TestCanonStateSubscriptions},
        BlockReader, BlockReaderIdExt, ChainSpecProvider, EvmEnvProvider, StateProviderFactory,
    },
    rpc::builder::{
        RpcModuleBuilder, RpcModuleSelection, RpcServerConfig, RpcServerHandle,
        TransportRpcModuleConfig,
    },
    tasks::{
        pool::{BlockingTaskGuard, BlockingTaskPool},
        TokioTaskExecutor,
    },
    transaction_pool::test_utils::{testing_pool, TestPool},
};
use reth_node_ethereum::EthEvmConfig;
use reth_rpc::{
    eth::{
        cache::EthStateCache, gas_oracle::GasPriceOracle, FeeHistoryCache, FeeHistoryCacheConfig,
    },
    EthApi,
};
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde::Deserialize;
use serde_json::json;

use crate::BundlesApiExt;

#[derive(Deserialize, Debug)]
pub struct RpcErrResponse {
    pub jsonrpc: String,
    pub error: ErrResponse,
    pub id: i64,
}

#[derive(Deserialize, Debug)]
pub struct RpcResponse<T> {
    pub jsonrpc: String,
    pub result: T,
    pub id: i64,
}

pub fn generate_random_key() -> (SecretKey, Address) {
    let secret_key = SecretKey::new(&mut rand::thread_rng());
    let secp = Secp256k1::new();
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);
    let hash = keccak256(&public_key.serialize_uncompressed()[1..]);
    let address = Address::from_slice(&hash[12..]);
    (secret_key, address)
}

pub fn sign_transaction(secret_key: &SecretKey, transaction: Transaction) -> TransactionSigned {
    let tx_signature_hash = transaction.signature_hash();
    let signature = sign_message(B256::from_slice(secret_key.as_ref()), tx_signature_hash).unwrap();
    TransactionSigned::from_transaction_and_signature(transaction, signature)
}

pub async fn assert_validation_error(expected_err: String, response: Response) {
    let to_json = response.json::<RpcErrResponse>().await.unwrap();
    assert_eq!(to_json.error.message, expected_err);
}

#[derive(Deserialize, Debug)]
pub struct ErrResponse {
    pub code: i32,
    pub message: String,
}

pub struct RpcServerSetup {
    port: String,
    bundle_pool: BundlePool,
    pool: TestPool,
}

impl RpcServerSetup {
    pub async fn new(port: String, bundle_pool: BundlePool, pool: TestPool) -> Self {
        RpcServerSetup {
            port,
            bundle_pool,
            pool,
        }
    }

    pub async fn start(
        &self,
        eth_api: EthApi<MockEthProvider, TestPool, NoopNetwork, EthEvmConfig>,
    ) -> RpcServerHandle {
        let mock_provider = MockEthProvider::default();
        let pool = self.pool.clone();
        let rpc_builder = RpcModuleBuilder::default()
            .with_provider(mock_provider.clone())
            // Rest is just noops that do nothing
            .with_pool(pool.clone())
            .with_noop_network()
            .with_evm_config(EthEvmConfig::default())
            .with_executor(TokioTaskExecutor::default())
            .with_events(TestCanonStateSubscriptions::default());

        // Pick which namespaces to expose.
        let config =
            TransportRpcModuleConfig::default().with_http(RpcModuleSelection::Selection(vec![]));
        let mut server = rpc_builder.build(config);

        // Add a custom rpc namespace
        let rpc = BundlesApiExt {
            provider: mock_provider.clone(),
            bundles_pool: self.bundle_pool.clone(),
            pool: pool.clone(),
            blocking_pool_guard: BlockingTaskGuard::new(10),
            eth_api,
        };
        server.merge_configured(rpc.into_rpc()).unwrap();
        let address = format!("0.0.0.0:{}", self.port);
        // Start the server
        let server_args =
            RpcServerConfig::http(Default::default()).with_http_address(address.parse().unwrap());
        server_args.start(server).await.unwrap()
    }
}

#[derive(Clone)]
pub struct TestContext {
    rpc_server_handle: RpcServerHandle,
}

impl TestContext {
    pub fn new(rpc_server_handle: RpcServerHandle) -> Self {
        TestContext { rpc_server_handle }
    }

    pub async fn send_request(&self, rpc_method: String, params: serde_json::Value) -> Response {
        let url = self.rpc_server_handle.http_url().unwrap();
        let client = reqwest::Client::builder().build().unwrap();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());

        client
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
            .unwrap()
    }
}

pub struct EthApiTestBuilder;

impl EthApiTestBuilder {
    fn build<
        P: BlockReaderIdExt
            + BlockReader
            + ChainSpecProvider
            + EvmEnvProvider
            + StateProviderFactory
            + Unpin
            + Clone
            + 'static,
    >(
        provider: P,
    ) -> EthApi<P, TestPool, NoopNetwork, EthEvmConfig> {
        let cache = EthStateCache::spawn(
            provider.clone(),
            Default::default(),
            EthEvmConfig::default(),
        );
        let fee_history_cache =
            FeeHistoryCache::new(cache.clone(), FeeHistoryCacheConfig::default());
        EthApi::new(
            provider.clone(),
            testing_pool(),
            NoopNetwork::default(),
            cache.clone(),
            GasPriceOracle::new(provider, Default::default(), cache),
            ETHEREUM_BLOCK_GAS_LIMIT,
            BlockingTaskPool::build().expect("failed to build tracing pool"),
            fee_history_cache,
            EthEvmConfig::default(),
            None,
        )
    }

    pub fn prepare_with_mock_data(
        block: u64,
        count: u64,
        sender: Option<Address>,
    ) -> EthApi<MockEthProvider, TestPool, NoopNetwork, EthEvmConfig> {
        let mock_provider = MockEthProvider::default();
        let mut rng = rand::thread_rng();

        for i in (0..count).rev() {
            let hash = rng.gen();
            let gas_limit: u64 = rng.gen();
            let gas_used: u64 = rng.gen();
            let base_fee_per_gas: u64 = rng.gen();

            let current_block = block + i;

            let header = Header {
                number: current_block,
                gas_limit,
                gas_used,
                base_fee_per_gas: Some(base_fee_per_gas),
                excess_blob_gas: Some(0),
                blob_gas_used: Some(0),
                ..Default::default()
            };

            mock_provider.add_block(
                hash,
                Block {
                    header: header.clone(),
                    body: vec![],
                    ..Default::default()
                },
            );
            mock_provider.add_header(hash, header);

            if let Some(sender) = sender {
                mock_provider.add_account(sender, ExtendedAccount::new(0, U256::MAX));
            }
        }

        Self::build(mock_provider)
    }
}

#[derive(Clone)]
pub struct RpcTestSuite {
    test_context: TestContext,
    bundle_pool: BundlePool,
    pool: TestPool,
}

impl RpcTestSuite {
    pub async fn new(
        port: String,
        block: u64,
        count: u64,
        sender: Option<Address>,
        bundle_pool: Option<BundlePool>,
    ) -> Self {
        // Prepare the EthApi with mock data
        let eth_api = EthApiTestBuilder::prepare_with_mock_data(block, count, sender);

        let bundle_pool = bundle_pool.unwrap_or_default();
        let pool = testing_pool();

        // Setup RPC server
        let rpc_server_setup =
            RpcServerSetup::new(port.clone(), bundle_pool.clone(), pool.clone()).await;
        let rpc_server_handle = rpc_server_setup.start(eth_api).await;

        // Initialize the TestContext with the RPC server handle
        let test_context = TestContext::new(rpc_server_handle);

        RpcTestSuite {
            test_context,
            bundle_pool,
            pool,
        }
    }
    pub fn bundle_pool(&self) -> &BundlePool {
        &self.bundle_pool
    }
    pub fn pool(&self) -> &TestPool {
        &self.pool
    }

    pub async fn send_request(&self, rpc_method: String, params: serde_json::Value) -> Response {
        self.test_context.send_request(rpc_method, params).await
    }
}
