use crate::blst::public_key::BlsPublicKey;
use crate::blst::signature::BlsSignature;
use crate::build::Build;
use crate::relays::RelayName;
use crate::signer::BlsSigner;
use crate::{Bloom, Bytes, U64};
use chrono::Utc;
use reth::rpc::types::engine::ExecutionPayloadV3;
use reth::rpc::types::Withdrawal;
use reth_rpc_types::engine::{BlobsBundleV1, ExecutionPayloadEnvelopeV3};
use serde::{de, Serializer};
use serde_derive::{Deserialize, Serialize};
use serde_with::DisplayFromStr;
use serde_with::{serde, serde_as};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::{blst::SignedRoot, Address, B256, U256};

fn from_u64_to_str<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}

fn from_u256_to_str<S>(value: &U256, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let value_str = format!("{}", value.0);
    serializer.serialize_str(&value_str)
}

fn from_str_to_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: &str = serde::Deserialize::deserialize(deserializer)?;
    s.parse::<u64>()
        .map_err(|_| de::Error::custom("failed to parse"))
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, Default)]
pub struct Bid {
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub slot: u64,
    pub parent_hash: B256,
    pub block_hash: B256,
    pub builder_pubkey: BlsPublicKey,
    pub proposer_pubkey: BlsPublicKey,
    pub proposer_fee_recipient: Address,
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub gas_limit: u64,
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub gas_used: u64,
    #[serde(serialize_with = "from_u256_to_str")]
    pub value: U256,
}

impl SignedRoot for Bid {}

impl Bid {
    pub fn new(build: &Build, builder_pubkey: BlsPublicKey) -> Self {
        let Build {
            validator_info,
            payload,
            bid,
            ..
        } = build;
        let validator_info = validator_info.clone();
        Self {
            slot: validator_info.slot,
            parent_hash: payload.block().parent_hash.into(),
            block_hash: payload.block().hash().into(),
            gas_limit: payload.block().gas_limit,
            gas_used: payload.block().gas_used,
            value: (*bid).into(),
            builder_pubkey,
            proposer_pubkey: validator_info.proposer.clone(),
            proposer_fee_recipient: validator_info.fee_recipient.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Encode, Decode, PartialEq, Default)]
pub struct AlloyWithdrawal {
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub index: u64,
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub validator_index: u64,
    pub address: Address,
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub amount: u64,
}

impl From<Withdrawal> for AlloyWithdrawal {
    fn from(withdrawal: Withdrawal) -> Self {
        Self {
            index: withdrawal.index,
            validator_index: withdrawal.validator_index,
            address: withdrawal.address.into(),
            amount: withdrawal.amount,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Encode, Decode, PartialEq, Default)]
pub struct ExecutionPayload {
    pub parent_hash: B256,
    pub fee_recipient: Address,
    pub state_root: B256,
    pub receipts_root: B256,
    pub logs_bloom: Bloom,
    pub prev_randao: B256,
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub block_number: u64,
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub gas_limit: u64,
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub gas_used: u64,
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub timestamp: u64,
    pub extra_data: Bytes,
    #[serde(serialize_with = "from_u256_to_str")]
    pub base_fee_per_gas: U256,
    pub block_hash: B256,
    pub transactions: Vec<Bytes>,
    pub withdrawals: Vec<AlloyWithdrawal>,
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub blob_gas_used: u64,
    #[serde(
        serialize_with = "from_u64_to_str",
        deserialize_with = "from_str_to_u64"
    )]
    pub excess_blob_gas: u64,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct AlloyExecutionPayload(ExecutionPayloadV3);

impl From<ExecutionPayloadV3> for AlloyExecutionPayload {
    fn from(payload: ExecutionPayloadV3) -> Self {
        Self(payload)
    }
}

impl AsRef<ExecutionPayloadV3> for AlloyExecutionPayload {
    fn as_ref(&self) -> &ExecutionPayloadV3 {
        &self.0
    }
}

impl From<u64> for U64 {
    fn from(uinteger: u64) -> Self {
        Self(reth::primitives::U64::from(uinteger))
    }
}

impl From<u64> for U256 {
    fn from(uinteger: u64) -> Self {
        Self(reth::primitives::U256::from(uinteger))
    }
}

impl From<AlloyExecutionPayload> for ExecutionPayload {
    fn from(alloy: AlloyExecutionPayload) -> Self {
        let inner = alloy.0;
        let payload = inner.payload_inner.payload_inner;

        // Flatten the ExecutionPayloadV3 into ExecutionPayload
        ExecutionPayload {
            parent_hash: payload.parent_hash.into(),
            fee_recipient: payload.fee_recipient.into(),
            state_root: payload.state_root.into(),
            receipts_root: payload.receipts_root.into(),
            logs_bloom: payload.logs_bloom.into(),
            prev_randao: payload.prev_randao.into(),
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data.into(),
            base_fee_per_gas: payload.base_fee_per_gas.into(),
            block_hash: payload.block_hash.into(),
            transactions: payload.transactions.into_iter().map(Bytes::from).collect(),
            withdrawals: inner
                .payload_inner
                .withdrawals
                .into_iter()
                .map(AlloyWithdrawal::from)
                .collect(),
            blob_gas_used: inner.blob_gas_used,
            excess_blob_gas: inner.excess_blob_gas,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Encode, Decode, PartialEq, Debug)]
pub struct SignedBid {
    pub message: Bid,
    pub execution_payload: ExecutionPayload,
    pub blobs_bundle: BlobsBundleV1,
    pub signature: BlsSignature,
}

impl Default for SignedBid {
    fn default() -> Self {
        SignedBid {
            blobs_bundle: BlobsBundleV1 {
                commitments: vec![],
                proofs: vec![],
                blobs: vec![],
            },
            execution_payload: Default::default(),
            signature: Default::default(),
            message: Default::default(),
        }
    }
}

impl SignedBid {
    pub fn new(
        message: Bid,
        payload_envelope: ExecutionPayloadEnvelopeV3,
        signer: BlsSigner,
    ) -> Self {
        let signature = signer.sign(&mut message.clone());
        let blobs_bundle = payload_envelope.blobs_bundle;
        let exec_payload: AlloyExecutionPayload = payload_envelope.execution_payload.into();
        let execution_payload = ExecutionPayload::from(exec_payload);
        Self {
            message,
            execution_payload,
            blobs_bundle,
            signature,
        }
    }

    pub fn to_trace(&self) -> BidTrace {
        BidTrace {
            slot: self.message.slot,
            parent_hash: self.message.parent_hash,
            block_hash: self.message.block_hash,
            builder_pubkey: self.message.builder_pubkey.clone(),
            proposer_pubkey: self.message.proposer_pubkey.clone(),
            proposer_fee_recipient: self.message.proposer_fee_recipient,
            gas_limit: self.message.gas_limit,
            gas_used: self.message.gas_used,
            value: self.message.value,
            num_tx: self.execution_payload.transactions.len() as u64,
            block_number: self.execution_payload.block_number,
            timestamp: self.execution_payload.timestamp,
            timestamp_ms: Utc::now().timestamp_millis() as u64,
            optimistic_submission: Some(true),
        }
    }

    pub fn block_number(&self) -> u64 {
        self.execution_payload.block_number
    }
}

#[serde_as]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct BidTrace {
    #[serde_as(as = "DisplayFromStr")]
    pub slot: u64,
    pub parent_hash: B256,
    pub block_hash: B256,
    pub builder_pubkey: BlsPublicKey,
    pub proposer_pubkey: BlsPublicKey,
    pub proposer_fee_recipient: Address,
    #[serde_as(as = "DisplayFromStr")]
    pub gas_limit: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub gas_used: u64,
    pub value: U256,
    #[serde_as(as = "DisplayFromStr")]
    pub num_tx: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub block_number: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub timestamp: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub timestamp_ms: u64,
    pub optimistic_submission: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedSignedBuilderBid {
    pub version: DataVersion,
    pub capella: Option<SignedBuilderBid>,
}

type DataVersion = u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedBuilderBid {
    pub message: BuilderBid,
    pub signature: BlsSignature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderBid {
    pub header: ExecutionPayloadV3,
    pub value: U256,
    pub pubkey: BlsPublicKey,
}

pub type RelayBidTrace = (RelayName, Vec<BidTrace>);

impl BidTrace {
    pub fn has_hash(&self, block_hash: &String) -> bool {
        self.block_hash.to_string().as_str() == block_hash
    }
}

#[cfg(test)]
mod tests {
    use reth::primitives::{Block, U256};
    use reth_payload_builder::{EthBuiltPayload, PayloadId};
    use reth_rpc_types::engine::ExecutionPayloadEnvelopeV3;
    use ssz::{Decode, Encode};

    use super::{AlloyExecutionPayload, Bid, SignedBid};
    use crate::{
        bid::ExecutionPayload,
        blst::{
            public_key::BlsPublicKey, secret_key::BlsSecretKey, signature::BlsSignature, SignedRoot,
        },
        build::{Build, BuildId},
        signer::BlsSigner,
        validator::ValidatorScheduleSlotInfo,
        B256,
    };
    use reth::revm::primitives::FixedBytes;
    use serde_json;
    use tree_hash::Hash256;

    fn random_b256() -> B256 {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
        B256(FixedBytes::from(bytes))
    }

    fn bid(sk: BlsSecretKey) -> (Bid, EthBuiltPayload) {
        let best_payload = EthBuiltPayload::new(
            PayloadId::new([0; 8]),
            Block::default().seal_slow(),
            U256::ZERO,
        );
        let build = Build {
            id: BuildId::random(),
            validator_info: ValidatorScheduleSlotInfo {
                proposer: sk.public_key(),
                gas_limit: 30_000,
                ..Default::default()
            },
            payload: best_payload.clone(),
            bid: Default::default(),
        };

        (Bid::new(&build, sk.public_key()), best_payload)
    }
    fn signed_bid() -> (SignedBid, [u8; 32]) {
        let sk = BlsSecretKey::random();

        let (bid, bp) = bid(sk.clone());

        let signer = BlsSigner::new(sk, Default::default());
        let domain = signer.compute_builder_domain();

        let v3_envelope = ExecutionPayloadEnvelopeV3::from(bp);

        (SignedBid::new(bid, v3_envelope, signer), domain)
    }
    #[test]
    fn test_serialize_deseralize() {
        let builder_pubkey = BlsSecretKey::random().public_key();
        let proposer_pubkey = BlsSecretKey::random().public_key();
        let bid = Bid {
            slot: 7769266,
            parent_hash: random_b256(),
            block_hash: random_b256(),
            builder_pubkey,
            proposer_pubkey,
            proposer_fee_recipient: reth::primitives::Address::random().into(),
            gas_limit: 30000000,
            gas_used: 12770146,
            value: U256::from(49137063656877759u64).into(),
        };

        let serialized = serde_json::to_string(&bid).unwrap();
        let deserialized: Bid = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, bid);
    }

    #[test]
    fn test_bid_instance() {
        let sk = BlsSecretKey::random();
        let (bid, _) = bid(sk);
        assert_eq!(bid.slot, 0);
        let encoded = bid.as_ssz_bytes();
        let decoded: Bid = Bid::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(decoded, bid);
    }
    #[test]
    fn test_v3_payload_instance() {
        let sk = BlsSecretKey::random();
        let (_, bp) = bid(sk);
        let v3_envelope = ExecutionPayloadEnvelopeV3::from(bp);
        let ep = AlloyExecutionPayload(v3_envelope.execution_payload);
        let v3 = ExecutionPayload::from(ep);
        let encoded = v3.as_ssz_bytes();
        let decoded: ExecutionPayload = ExecutionPayload::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(decoded, v3);
    }
    #[test]
    fn test_signed_bid() {
        let (signed_bid, domain) = signed_bid();
        assert_eq!(signed_bid.message.slot, 0);

        let is_verified = signed_bid.signature.verify(
            &signed_bid.message.builder_pubkey,
            signed_bid.message.signing_root(domain.into()),
        );
        assert!(is_verified);
    }
    #[test]
    fn test_bid_trace() {
        let (signed_bid, _) = signed_bid();
        let bid_trace = signed_bid.to_trace();
        assert_eq!(bid_trace.slot, 0);
    }

    #[test]
    fn can_ssz_signed_bid() {
        let (signed_bid, _) = signed_bid();

        let ssz_bytes = signed_bid.as_ssz_bytes();

        let decoded: SignedBid = SignedBid::from_ssz_bytes(&ssz_bytes).unwrap();
        assert_eq!(decoded, signed_bid);
    }

    #[test]
    fn can_parse_signature() {
        let sk = BlsSecretKey::random();
        let signing_root = Hash256::from_slice(&[0u8; 32]);
        let bls_sig = sk.sign(signing_root);
        let ssz_bytes = bls_sig.as_ssz_bytes();
        let decoded: BlsSignature = BlsSignature::from_ssz_bytes(&ssz_bytes).unwrap();
        assert_eq!(decoded, bls_sig);
    }
    #[test]
    fn can_parse_pubkey() {
        let secret = BlsSecretKey::random();
        let pk = secret.public_key();
        let ssz_bytes = pk.as_ssz_bytes();
        let decoded: BlsPublicKey = BlsPublicKey::from_ssz_bytes(&ssz_bytes).unwrap();
        assert_eq!(decoded, pk);
    }
}
