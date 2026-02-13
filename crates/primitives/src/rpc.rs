use reth::primitives::{Address, Bytes, B256, U64};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EthSendBundle {
    /// A list of hex-encoded signed transactions
    pub txs: Vec<Bytes>,
    /// hex-encoded block number for which this bundle is valid
    pub block_number: U64,
    /// unix timestamp when this bundle becomes active
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_timestamp: Option<u64>,
    /// unix timestamp how long this bundle stays valid
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_timestamp: Option<u64>,
    /// list of hashes of possibly reverting txs
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reverting_tx_hashes: Vec<B256>,
    /// UUID that can be used to cancel/replace this bundle
    #[serde(rename = "replacementUuid", skip_serializing_if = "Option::is_none")]
    pub replacement_uuid: Option<Uuid>,
    /// Percentage (from 0 to 100) of the ETH reward of the transaction at refundIndex
    #[serde(rename = "refundPercent", skip_serializing_if = "Option::is_none")]
    pub refund_percent: Option<u64>,
    /// Index of transaction in txs to be used for refund calculation, default is last transaction
    #[serde(rename = "refundIndex", skip_serializing_if = "Option::is_none")]
    pub refund_index: Option<usize>,
    /// Recipient address of refund, default is sender of last transaction
    #[serde(rename = "refundRecipient", skip_serializing_if = "Option::is_none")]
    pub refund_recipient: Option<Address>,
}
