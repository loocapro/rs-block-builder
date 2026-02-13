use reth::{
    primitives::{ChainSpec, SealedBlock},
    revm::primitives::{BlockEnv, Bytes, CfgEnv},
};
use reth_payload_builder::PayloadBuilderAttributes;
use std::sync::Arc;
/// Static config for how to build a payload.
#[derive(Clone, Debug)]
pub struct PayloadConfig {
    /// Pre-configured block environment.
    pub initialized_block_env: BlockEnv,
    /// Configuration for the environment.
    pub initialized_cfg: CfgEnv,
    /// The parent block.
    pub parent_block: Arc<SealedBlock>,
    /// Block extra data.
    pub extra_data: Bytes,
    /// Requested attributes for the payload.
    pub attributes: PayloadBuilderAttributes,
    /// The chain spec.
    pub chain_spec: Arc<ChainSpec>,
}

impl PayloadConfig {
    /// Create new payload config.
    pub fn new(
        parent_block: Arc<SealedBlock>,
        extra_data: Bytes,
        attributes: PayloadBuilderAttributes,
        chain_spec: Arc<ChainSpec>,
    ) -> Self {
        // configure evm env based on parent block
        // this will be used to configure the base_fee for our payload
        let (initialized_cfg, initialized_block_env) =
            attributes.cfg_and_block_env(&chain_spec, &parent_block);

        Self {
            initialized_block_env,
            initialized_cfg,
            parent_block,
            extra_data,
            attributes,
            chain_spec,
        }
    }
}
