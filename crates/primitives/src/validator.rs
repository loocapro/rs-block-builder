/// This code snippet defines several structs and implements methods for manipulating and storing validator slot info. It includes a `ValidatorScheduleSlotInfo` struct with fields for proposer public key, fee recipient address, gas limit, slot number, and relays URL. The code also includes a `ValidatorSchedule` struct that holds a map of payload slots pointing to `ValidatorScheduleSlotInfo` instances. The code provides methods for creating a new `ValidatorSchedule` instance, getting `ValidatorScheduleSlotInfo` by slot number, and merging relays for the same slot. The code also includes tests for validating the functionality of the code.
use crate::blst::{public_key::BlsPublicKey, signature::BlsSignature};
use reth::primitives::Address;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_with::{serde, serde_as, DisplayFromStr};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ValidatorScheduleSlotInfo {
    /// Proposer PubKey
    pub proposer: BlsPublicKey,
    /// Fee Recipient
    pub fee_recipient: Address,
    /// Gas Limit
    pub gas_limit: u64,
    /// Slot of ethereum epochs
    pub slot: u64,
}

impl ValidatorScheduleSlotInfo {
    /// Suggests an appropriate gas limit for a block based on the parent block's gas limit.
    ///
    /// This method aims to adjust the gas limit closer to the desired target set by the `gas_limit` field
    /// of `ValidatorScheduleSlotInfo`. It increases the limit if it's below the target and decreases it
    /// if it's above, ensuring that it doesn't fall below a minimum threshold.
    ///
    /// # Arguments
    /// * `parent_gas_limit` - The gas limit of the parent block.
    ///
    /// # Returns
    /// * `u64` - The adjusted gas limit for the current block.
    pub fn suggest_gas_limit(&self, parent_gas_limit: u64) -> u64 {
        const MIN_GAS_LIMIT: u64 = 5000;
        const GAS_LIMIT_BOUND_DIVISOR: u64 = 1024;

        if parent_gas_limit == 0 {
            return parent_gas_limit;
        }

        let delta = parent_gas_limit / GAS_LIMIT_BOUND_DIVISOR - 1;
        let mut limit = parent_gas_limit;
        let mut desired_limit = self.gas_limit;

        if desired_limit < MIN_GAS_LIMIT {
            desired_limit = MIN_GAS_LIMIT
        }

        if limit < desired_limit {
            limit = parent_gas_limit + delta;
            if limit > desired_limit {
                limit = desired_limit;
            }
            return limit;
        }
        if limit > desired_limit {
            limit = parent_gas_limit - delta;
            if limit < desired_limit {
                limit = desired_limit
            }
        }
        limit
    }
}

impl From<ValidatorRegistrationResponse> for ValidatorScheduleSlotInfo {
    fn from(data: ValidatorRegistrationResponse) -> Self {
        ValidatorScheduleSlotInfo {
            proposer: data.entry.message.pubkey,
            fee_recipient: data.entry.message.fee_recipient,
            gas_limit: data.entry.message.gas_limit,
            slot: data.slot,
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidatorRegistrationResponse {
    #[serde_as(as = "DisplayFromStr")]
    pub slot: u64,
    pub validator_index: Option<String>,
    pub entry: SignedValidatorRegistration,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SignedValidatorRegistration {
    pub message: Registration,
    pub signature: BlsSignature,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Registration {
    pub fee_recipient: Address,
    #[serde_as(as = "DisplayFromStr")]
    pub gas_limit: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub timestamp: u64,
    pub pubkey: BlsPublicKey,
}

/// Holds a map of payload slot pointing to ValidatorScheduleSlotInfo
#[derive(Clone, Debug, Default)]
pub struct ValidatorSchedule(HashMap<u64, ValidatorScheduleSlotInfo>);

impl ValidatorSchedule {
    pub fn new(v_info_vec: Vec<ValidatorScheduleSlotInfo>) -> Self {
        let mut v_relays: HashMap<u64, ValidatorScheduleSlotInfo> = HashMap::new();

        for v_info in v_info_vec {
            let slot = v_info.slot;
            v_relays.entry(slot).or_insert(v_info);
        }

        Self(v_relays)
    }

    pub fn get_by_slot(&self, slot: u64) -> Option<ValidatorScheduleSlotInfo> {
        self.0.get(&slot).cloned()
    }
}

impl AsRef<HashMap<u64, ValidatorScheduleSlotInfo>> for ValidatorSchedule {
    fn as_ref(&self) -> &HashMap<u64, ValidatorScheduleSlotInfo> {
        &self.0
    }
}

#[cfg(test)]
mod tests {

    use reth::primitives::Address;

    use crate::blst::secret_key::BlsSecretKey;

    use super::*;

    #[test]
    fn test_validators_insertion() {
        let v_info_1 = ValidatorScheduleSlotInfo {
            proposer: BlsSecretKey::random().public_key(),
            fee_recipient: Address::random(),
            gas_limit: 100,
            slot: 1,
        };
        let v_info_2 = ValidatorScheduleSlotInfo {
            proposer: BlsSecretKey::random().public_key(),
            fee_recipient: Address::default(),
            gas_limit: 200,
            slot: 2,
        };

        let validators = ValidatorSchedule::new(vec![v_info_1.clone(), v_info_2.clone()]);

        assert_eq!(validators.as_ref().get(&1), Some(&v_info_1));
        assert_eq!(validators.as_ref().get(&2), Some(&v_info_2));
    }

    fn test_scenario(p_gas_limit: u64, gas_limit: u64, expected_limit: u64) {
        let v_info = ValidatorScheduleSlotInfo {
            proposer: BlsSecretKey::random().public_key(),
            fee_recipient: Address::random(),
            gas_limit,
            slot: 1,
        };
        let limit = v_info.suggest_gas_limit(p_gas_limit);
        assert_eq!(limit, expected_limit);
    }

    #[test]
    fn test_calc_gas_limit() {
        let test_cases = vec![
            (20_000_000, 20_019_530, 19_980_470),
            (40_000_000, 40_039_061, 39_960_939),
        ];

        for (p_gas_limit, max, min) in test_cases {
            test_scenario(p_gas_limit, 2 * p_gas_limit, max);
            test_scenario(p_gas_limit, 0, min);
            test_scenario(p_gas_limit, p_gas_limit - 1, p_gas_limit - 1);
            test_scenario(p_gas_limit, p_gas_limit + 1, p_gas_limit + 1);
            test_scenario(p_gas_limit, p_gas_limit, p_gas_limit);
        }
    }
}
