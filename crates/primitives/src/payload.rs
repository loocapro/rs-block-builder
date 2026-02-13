use crate::validator::ValidatorScheduleSlotInfo;
use reth::{
    primitives::{Address, Block, ChainSpec, SealedBlock, U256},
    revm::primitives::{BlockEnv, Bytes, CfgEnvWithHandlerCfg},
};
use reth_node_api::engine::traits::PayloadBuilderAttributes;
use reth_payload_builder::EthPayloadBuilderAttributes;
use secp256k1::SecretKey;
use std::fmt::{self, Debug};
use std::sync::Arc;

/// Static payload configs
#[derive(Debug, Clone)]
pub struct PayloadConfig {
    /// The chain spec.
    pub chain_spec: Arc<ChainSpec>,
    /// The address of the builder.
    pub builder_address: Address,
    /// The secret key of the builder.
    pub secret_key: SecretKey,
    /// Block extra data.
    pub extra_data: Bytes,
}
impl Default for PayloadConfig {
    fn default() -> Self {
        PayloadConfig {
            chain_spec: Arc::new(ChainSpec::default()),
            builder_address: Address::ZERO,
            secret_key: SecretKey::new(&mut rand::thread_rng()),
            extra_data: Bytes::default(),
        }
    }
}

/// Configurations used during builds.
#[derive(Clone)]
pub struct BuildConfig {
    /// Build slot
    pub slot: u64,
    /// Requested attributes for the payload.
    pub attributes: EthPayloadBuilderAttributes,
    /// Validator info for slot
    pub validator_info: ValidatorScheduleSlotInfo,
    /// The parent block.
    pub parent_block: Arc<SealedBlock>,
    /// Current block number
    pub block_number: u64,
    /// Pre-configured block environment.
    pub initialized_block_env: BlockEnv,
    /// Configuration for the environment.
    pub initialized_cfg: CfgEnvWithHandlerCfg,
    /// Block gas limit
    pub block_gas_limit: u64,
    /// Base fee
    pub basefee: U256,
}

impl BuildConfig {
    pub fn new(
        chain_spec: &ChainSpec,
        attributes: EthPayloadBuilderAttributes,
        parent_block: Arc<SealedBlock>,
        slot: u64,
        validator_info: ValidatorScheduleSlotInfo,
    ) -> Self {
        // configure evm env based on parent block
        // this will be used to configure the base_fee for our payload
        let (initialized_cfg, initialized_block_env) =
            attributes.cfg_and_block_env(chain_spec, &parent_block);
        let block_number = initialized_block_env.number.to::<u64>();

        BuildConfig {
            slot,
            attributes,
            validator_info: validator_info.clone(),
            parent_block: parent_block.clone(),
            block_number,
            initialized_block_env: initialized_block_env.clone(),
            initialized_cfg,
            block_gas_limit: validator_info.suggest_gas_limit(parent_block.gas_limit),
            basefee: initialized_block_env.basefee,
        }
    }
    /// Logs helper
    pub fn to_log(&self) -> String {
        format!("slot = {}, block_number = {}", self.slot, self.block_number)
    }

    pub fn block_env_with_coinbase(&mut self, builder_address: Address) -> BlockEnv {
        self.initialized_block_env.coinbase = builder_address;
        self.initialized_block_env.clone()
    }
}

impl fmt::Debug for BuildConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            " slot = {}, block_number = {}",
            self.slot, self.block_number
        )
    }
}

impl From<EthPayloadBuilderAttributes> for BuildConfig {
    fn from(attributes: EthPayloadBuilderAttributes) -> Self {
        BuildConfig::new(
            &ChainSpec::default(),
            attributes,
            Arc::new(Block::default().seal_slow()),
            0,
            ValidatorScheduleSlotInfo::default(),
        )
    }
}
