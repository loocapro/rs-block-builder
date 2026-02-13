use reth::primitives::{ChainSpec, B64, U256, U64};
use reth_payload_builder::{EthBuiltPayload, EthPayloadBuilderAttributes};
use std::fmt::{self, Debug};
use std::sync::Arc;

use crate::validator::ValidatorScheduleSlotInfo;

pub mod log;

/// Max possible gas limit spent by the payment tx
/// A normal transfer is 21000, but if the recipient is a contract, it could be higher
pub const MAX_BUILDER_TRANSFER_GAS_LIMIT: u64 = 32000;

/// And 8-byte identifier for build objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BuildId(B64);

impl BuildId {
    /// Creates a random build id
    pub fn random() -> Self {
        Self(B64::random())
    }
}

impl std::fmt::Display for BuildId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Clone)]
pub struct Build {
    /// Build identifier
    pub id: BuildId,
    pub validator_info: ValidatorScheduleSlotInfo,
    /// Built payload
    pub payload: EthBuiltPayload,
    /// Bid amount
    pub bid: U256,
}

impl Build {
    pub fn as_block_number_and_ts(&self) -> (U64, u64) {
        let block_number = U64::from(self.payload.block().number);
        let timestamp = self.payload.block().timestamp;
        (block_number, timestamp)
    }
}

impl fmt::Debug for Build {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (block_number, timestamp) = self.as_block_number_and_ts();
        let slot = self.validator_info.slot;
        write!(
            f,
            "build_id = {:?}, timestamp = {:?}, slot = {}, block_number = {}",
            self.id, timestamp, slot, block_number
        )
    }
}
pub mod test_utils {
    use std::time::{SystemTime, UNIX_EPOCH};

    use reth::primitives::{Address, Block, Header, Withdrawals, B256};
    use reth_payload_builder::PayloadId;

    use crate::{blst::secret_key::BlsSecretKey, payload::BuildConfig};

    use super::*;

    pub fn build_test_config(payload_id: PayloadId) -> BuildConfig {
        let parent_block = Block {
            header: Header {
                gas_limit: 30_000_000,
                excess_blob_gas: Some(0),
                blob_gas_used: Some(0),
                ..Default::default()
            },
            ..Default::default()
        }
        .seal_slow();

        let attributes = EthPayloadBuilderAttributes {
            id: payload_id,
            parent: parent_block.hash(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            suggested_fee_recipient: Address::random(),
            prev_randao: B256::random(),
            withdrawals: Withdrawals::new(vec![]),
            parent_beacon_block_root: None,
        };
        let proposer = BlsSecretKey::random();
        let validator_info = ValidatorScheduleSlotInfo {
            proposer: proposer.public_key(),
            gas_limit: parent_block.gas_limit,
            ..Default::default()
        };

        BuildConfig::new(
            &ChainSpec::default(),
            attributes,
            Arc::new(parent_block),
            0,
            validator_info,
        )
    }
}
