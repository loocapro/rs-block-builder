use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use reth::{
    primitives::{Address, B256},
    rpc::types::engine::PayloadAttributes,
};
use reth_payload_builder::EthPayloadBuilderAttributes;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
/// A response from the consensus layer that is versioned by the fork name
pub struct ForkVersionedResponse<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<ForkName>,
    pub data: T,
}

impl ForkVersionedResponse<SseExtendedPayloadAttributes> {
    /// Converts from CL types to reth types
    pub fn into_reth_payload_attributes(&self) -> EthPayloadBuilderAttributes {
        let pl_attr = PayloadAttributes {
            timestamp: self.data.payload_attributes.timestamp,
            prev_randao: self.data.payload_attributes.prev_randao,
            suggested_fee_recipient: self.data.payload_attributes.suggested_fee_recipient,
            withdrawals: self
                .data
                .payload_attributes
                .withdrawals
                .to_reth_withdrawals(),
            parent_beacon_block_root: Some(self.data.parent_block_root),
        };

        EthPayloadBuilderAttributes::new(self.data.parent_block_hash, pl_attr)
    }
}

/// TODO: Import from lighthouse once it does not break build
/// Server side events from the CL types wrapper
#[serde_as]
#[derive(Clone, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
pub struct SsePayloadAttributes {
    #[serde_as(as = "DisplayFromStr")]
    pub timestamp: u64,
    pub prev_randao: B256,
    pub suggested_fee_recipient: Address,
    pub withdrawals: Vec<LighthouseWithdrawal>,
}

/// Reth expects validatorIndex in PayloadAttributes
/// but lighthouse uses validator_index
/// so we cant deseralize directly into PayloadAttributes
/// but we have to go through a custom type
pub struct Withdrawal(reth::rpc::types::Withdrawal);

impl AsRef<reth::rpc::types::Withdrawal> for Withdrawal {
    fn as_ref(&self) -> &reth::rpc::types::Withdrawal {
        &self.0
    }
}

impl From<&LighthouseWithdrawal> for Withdrawal {
    fn from(withdrawal: &LighthouseWithdrawal) -> Self {
        Withdrawal(reth::rpc::types::Withdrawal {
            index: withdrawal.index,
            validator_index: withdrawal.validator_index,
            amount: withdrawal.amount,
            address: withdrawal.address,
        })
    }
}

#[serde_as]
#[derive(Clone, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
pub struct LighthouseWithdrawal {
    /// Monotonically increasing identifier issued by consensus layer.
    #[serde_as(as = "DisplayFromStr")]
    pub index: u64,
    /// Index of validator associated with withdrawal.
    #[serde_as(as = "DisplayFromStr")]
    pub validator_index: u64,
    /// Target address for withdrawn ether.
    pub address: Address,
    /// Value of the withdrawal in gwei.
    #[serde_as(as = "DisplayFromStr")]
    pub amount: u64,
}
trait Converter {
    fn to_reth_withdrawals(&self) -> Option<Vec<reth::rpc::types::Withdrawal>>;
}

impl Converter for Vec<LighthouseWithdrawal> {
    fn to_reth_withdrawals(&self) -> Option<Vec<reth::rpc::types::Withdrawal>> {
        if self.is_empty() {
            None
        } else {
            Some(
                self.iter()
                    .map(|lighthouse_withdrawal| {
                        let withdrawal: Withdrawal = Withdrawal::from(lighthouse_withdrawal);
                        withdrawal.0
                    })
                    .collect(),
            )
        }
    }
}

#[serde_as]
#[derive(PartialEq, Debug, Deserialize, Serialize, Clone)]
pub struct SseExtendedPayloadAttributesGeneric<T> {
    #[serde_as(as = "DisplayFromStr")]
    pub proposal_slot: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub proposer_index: u64,
    pub parent_block_root: B256,
    #[serde_as(as = "DisplayFromStr")]
    pub parent_block_number: u64,
    pub parent_block_hash: B256,
    pub payload_attributes: T,
}

pub type SseExtendedPayloadAttributes = SseExtendedPayloadAttributesGeneric<SsePayloadAttributes>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
#[serde(into = "String")]
pub enum ForkName {
    Base,
    Altair,
    Merge,
    Capella,
    Deneb,
}

impl FromStr for ForkName {
    type Err = String;

    fn from_str(fork_name: &str) -> Result<Self, String> {
        Ok(match fork_name.to_lowercase().as_ref() {
            "phase0" | "base" => ForkName::Base,
            "altair" => ForkName::Altair,
            "bellatrix" | "merge" => ForkName::Merge,
            "capella" => ForkName::Capella,
            "deneb" => ForkName::Deneb,
            _ => return Err(format!("unknown fork name: {}", fork_name)),
        })
    }
}

impl Display for ForkName {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            ForkName::Base => "phase0".fmt(f),
            ForkName::Altair => "altair".fmt(f),
            ForkName::Merge => "bellatrix".fmt(f),
            ForkName::Capella => "capella".fmt(f),
            ForkName::Deneb => "deneb".fmt(f),
        }
    }
}

impl From<ForkName> for String {
    fn from(fork: ForkName) -> String {
        fork.to_string()
    }
}

impl TryFrom<String> for ForkName {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_str(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_lighthouse_withdrawal() -> LighthouseWithdrawal {
        LighthouseWithdrawal {
            index: 1,
            validator_index: 2,
            address: Address::random(),
            amount: 100,
        }
    }

    #[test]
    fn test_convert_with_empty_vec() {
        let withdrawals: Vec<LighthouseWithdrawal> = vec![];
        assert!(withdrawals.to_reth_withdrawals().is_none());
    }

    #[test]
    fn test_convert_with_non_empty_vec() {
        let withdrawals: Vec<LighthouseWithdrawal> = vec![mock_lighthouse_withdrawal()];
        let converted = withdrawals.to_reth_withdrawals().unwrap();
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].index, 1);
        assert_eq!(converted[0].validator_index, 2);
        assert_eq!(converted[0].amount, 100);
    }
    #[test]
    fn test_convert_with_multiple_elements() {
        let withdrawals = vec![mock_lighthouse_withdrawal(), mock_lighthouse_withdrawal()];
        let converted = withdrawals.to_reth_withdrawals().unwrap();
        assert_eq!(converted.len(), 2);
    }
}
