use builder_primitives::build::MAX_BUILDER_TRANSFER_GAS_LIMIT;
use builder_primitives::rpc::EthSendBundle;
use reth::primitives::keccak256;
use reth::primitives::Address;
use reth::primitives::Bytes;
use reth::primitives::PooledTransactionsElementEcRecovered;
use reth::primitives::TransactionSigned;
use reth::primitives::TransactionSignedEcRecovered;
use reth::primitives::B256;
use reth::primitives::U256;
use reth::primitives::U64;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, PartialEq, Clone, Default)]
pub struct Bundle {
    pub hash: B256,
    /// A list of hex-encoded signed transactions
    pub txs: Vec<Bytes>,
    /// hex-encoded block number for which this bundle is valid
    pub block_number: U64,
    /// unix timestamp when this bundle becomes active
    pub min_timestamp: Option<u64>,
    /// unix timestamp how long this bundle stays valid
    pub max_timestamp: Option<u64>,
    /// list of hashes of possibly reverting txs
    pub reverting_tx_hashes: Vec<B256>,
    /// UUID that can be used to cancel/replace this bundle
    pub replacement_uuid: Option<Uuid>,
    /// Bundle refund from refund fields
    pub refund: Option<BundleRefund>,
}

impl Bundle {
    /// Decodes and recovers transactions for a collection of bundles,
    /// filtering out any bundles with incomplete transaction recoveries.
    pub fn recover_bundles(bundles: Vec<Arc<Self>>) -> Vec<RecoveredBundle> {
        bundles
            .into_iter()
            .filter_map(|bundle| {
                bundle
                    .txs
                    .clone()
                    .into_iter()
                    .map(|tx| {
                        let mut tx_bytes: &[u8] = tx.as_ref();
                        TransactionSigned::decode_enveloped(&mut tx_bytes)
                            .ok()
                            .and_then(|tx| tx.into_ecrecovered())
                    })
                    .collect::<Option<Vec<TransactionSignedEcRecovered>>>()
                    .map(|txs| RecoveredBundle::new(bundle.clone(), txs))
            })
            .collect()
    }

    /// Creates a bundle hash from a list of transactions
    pub fn create_hash(txs: &Vec<PooledTransactionsElementEcRecovered>) -> B256 {
        let mut bundle_hash: Vec<u8> = Vec::with_capacity(B256::len_bytes() * txs.len());
        for tx in txs {
            bundle_hash.extend_from_slice(tx.hash().as_slice());
        }
        keccak256(&bundle_hash)
    }
    /// Checks if a bundle is within the specified timestamp.
    /// Returns true if there is no min/max timestamp set on the bundle
    pub(crate) fn is_within_timestamp(&self, timestamp: u64) -> bool {
        let is_above_min = self
            .min_timestamp
            .is_none_or(|min_ts| timestamp >= min_ts);
        let is_below_max = self
            .max_timestamp
            .is_none_or(|max_ts| timestamp <= max_ts);

        is_above_min && is_below_max
    }

    /// Checks if a bundle is within the specified block number.
    pub(crate) fn is_within_block_number(&self, block_number: U64) -> bool {
        block_number == self.block_number
    }

    /// Checks whether the bundle is prunable from the pool.
    ///
    /// A bundle is considered prunable if its block number is less than or equal to the given block number
    /// or the given timestamp is outside its valid timestamp range (defined by min_timestamp and max_timestamp).
    pub(crate) fn is_prunable(&self, block_number: U64, timestamp: u64) -> bool {
        self.is_timestamp_out_of_bounds(timestamp) || self.block_number < block_number
    }

    /// Checks if a given timestamp falls outside the specified bounds.
    ///
    /// This function determines whether the provided timestamp is outside an optional range
    /// defined by `min_timestamp` and `max_timestamp`. It returns `true` if the timestamp is outside the range,
    /// and `false` otherwise.
    /// If either `min_timestamp` or `max_timestamp` is `None`, the timestamp is considered to be outside the range.
    fn is_timestamp_out_of_bounds(&self, timestamp: u64) -> bool {
        let is_before_min_ts = self
            .min_timestamp
            .is_some_and(|min_ts| timestamp < min_ts);
        let is_after_max_ts = self
            .max_timestamp
            .is_some_and(|max_ts| timestamp > max_ts);

        is_before_min_ts || is_after_max_ts
    }
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct BundleRefund {
    /// Percentage (from 0 to 100) of the ETH reward of the transaction at refundIndex
    pub percent: u64,
    /// Index of transaction in txs to be used for refund calculation, default is last transaction
    pub index: Option<usize>,
    /// Recipient address of refund, default is sender of last transaction
    pub recipient: Option<Address>,
}

impl BundleRefund {
    /// New bundle refund if and only if refundPercent is set, pass through optional index and
    /// recipient from EthSendBundle to Bundle
    pub fn new(
        percent: Option<u64>,
        index: Option<usize>,
        recipient: Option<Address>,
    ) -> Option<Self> {
        percent.map(|percent| Self {
            percent: percent.min(100),
            index,
            recipient,
        })
    }

    /// Calculates the refund for a simulated transaction.
    ///
    /// This function calculates the refund based on the provided `fees`, `coinbase_transfer`,
    /// and `base_fee`. It computes the refund amount using the formula:
    /// `(total_block_value - builder_transfer_cost) * refund_percent / 100`
    ///
    /// # Arguments
    ///
    /// * `fees` - The fees incurred during the simulated transaction.
    /// * `coinbase_transfer` - The amount transferred in the coinbase during the simulated transaction.
    /// * `base_fee` - The base fee used for calculating the refund.
    ///
    /// # Returns
    ///
    /// Returns the calculated refund amount as a `U256`.
    pub fn refund_amount(&self, fees: U256, coinbase_transfer: U256, base_fee: u64) -> U256 {
        fees.saturating_add(coinbase_transfer)
            .saturating_sub(U256::from(MAX_BUILDER_TRANSFER_GAS_LIMIT * base_fee))
            .saturating_mul(U256::from(self.percent))
            .checked_div(U256::from(100))
            .unwrap_or(U256::ZERO)
    }
}

/// From the `EthSendBundle` struct and `BundleHash`, we can derive the `Bundle` struct.
impl<'a> From<(&'a EthSendBundle, B256)> for Bundle {
    fn from(tuple: (&'a EthSendBundle, B256)) -> Self {
        let (bundle, hash) = tuple;

        Self {
            hash,
            txs: bundle.txs.clone(),
            block_number: bundle.block_number,
            min_timestamp: bundle.min_timestamp,
            max_timestamp: bundle.max_timestamp,
            reverting_tx_hashes: bundle.reverting_tx_hashes.clone(),
            replacement_uuid: bundle.replacement_uuid,
            refund: BundleRefund::new(
                bundle.refund_percent,
                bundle.refund_index,
                bundle.refund_recipient,
            ),
        }
    }
}

/// Recovered bundle that encapsulates recovered transactions and addition rpc fields
#[derive(Debug, Clone, Default)]
pub struct RecoveredBundle {
    /// A list of hex-encoded signed transactions
    txs: Vec<TransactionSignedEcRecovered>,
    /// list of hashes of possibly reverting txs
    pub reverting_tx_hashes: Vec<B256>,
    /// Arguments for bundle refund
    pub refund: Option<BundleRefund>,
}

impl RecoveredBundle {
    /// Create new RecoveredBundle struct
    pub fn new(bundle: Arc<Bundle>, txs: Vec<TransactionSignedEcRecovered>) -> Self {
        Self {
            txs,
            reverting_tx_hashes: bundle.reverting_tx_hashes.clone(),
            refund: bundle.refund.clone(),
        }
    }

    /// Get all tx hashes in bundle
    pub fn txs(&self) -> Vec<B256> {
        self.txs.iter().map(|tx| tx.hash()).collect()
    }

    /// Get all recovered txs in bundle
    pub fn recovered_txs(&self) -> Vec<TransactionSignedEcRecovered> {
        self.txs.clone()
    }

    /// Get all recovered txs in bundle
    pub fn recovered_txs_as_ref(&self) -> &Vec<TransactionSignedEcRecovered> {
        &self.txs
    }

    /// Check if bundle refund is set
    pub fn is_refund_bundle(&self) -> bool {
        self.refund.is_some()
    }

    /// Get effective refund transaction index
    /// If BundleRefund.index is none, default to last transaction index in txs
    pub fn refund_index(&self) -> Option<usize> {
        match self.refund.as_ref().and_then(|refund| refund.index) {
            Some(index) => Some(index),
            None => self.txs.len().checked_sub(1),
        }
    }

    /// Get effective refund recipient
    /// If BundleRefund.recipient is none, default to sender of last transaction
    pub fn refund_recipient(&self) -> Option<Address> {
        match self.refund.as_ref().and_then(|refund| refund.recipient) {
            Some(recipient) => Some(recipient),
            None => self
                .refund_index()
                .and_then(|i| self.txs.get(i).map(|tx| tx.signer())),
        }
    }
}
#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use reth::primitives::{B256, U64};
    use uuid::Uuid;

    // Helper function to create a test bundle
    fn create_test_bundle(
        block_number: U64,
        min_timestamp: Option<u64>,
        max_timestamp: Option<u64>,
    ) -> Bundle {
        Bundle {
            hash: B256::default(),
            txs: vec![],
            block_number,
            min_timestamp,
            max_timestamp,
            reverting_tx_hashes: vec![],
            replacement_uuid: Some(Uuid::new_v4()),
            refund: None,
        }
    }

    #[test]
    fn test_is_within_timestamp() {
        let bundle = create_test_bundle(U64::from(1), Some(1000), Some(2000));

        // Test within the timestamp range
        assert!(bundle.is_within_timestamp(1500));

        // Test below the minimum timestamp
        assert!(!bundle.is_within_timestamp(900));

        // Test above the maximum timestamp
        assert!(!bundle.is_within_timestamp(2100));

        // Test with no min_timestamp
        let no_min = create_test_bundle(U64::from(1), None, Some(2000));
        assert!(no_min.is_within_timestamp(900));

        // Test with no max_timestamp
        let no_max = create_test_bundle(U64::from(1), Some(1000), None);
        assert!(no_max.is_within_timestamp(2100));
    }

    #[test]
    fn test_is_within_block_number() {
        let bundle = create_test_bundle(U64::from(5), Some(1000), Some(2000));

        // Test within the block number
        assert!(bundle.is_within_block_number(U64::from(5)));

        // Test below the block number
        assert!(!bundle.is_within_block_number(U64::from(4)));

        // Test above the block number
        assert!(!bundle.is_within_block_number(U64::from(6)));
    }

    #[test]
    fn test_is_prunable() {
        let current_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Prunable by block number
        let prunable_by_block = create_test_bundle(
            U64::from(5),
            Some(current_timestamp - 1000),
            Some(current_timestamp + 1000),
        );
        assert!(prunable_by_block.is_prunable(U64::from(6), current_timestamp));

        // Prunable by timestamp
        let prunable_by_timestamp = create_test_bundle(
            U64::from(10),
            Some(current_timestamp - 2000),
            Some(current_timestamp - 1000),
        );
        assert!(prunable_by_timestamp.is_prunable(U64::from(5), current_timestamp));

        // Prunable by both block number and timestamp
        let prunable_by_both = create_test_bundle(
            U64::from(5),
            Some(current_timestamp - 2000),
            Some(current_timestamp - 1000),
        );
        assert!(prunable_by_both.is_prunable(U64::from(6), current_timestamp));

        // Non-prunable scenarios
        let non_prunable = create_test_bundle(
            U64::from(10),
            Some(current_timestamp - 1000),
            Some(current_timestamp + 1000),
        );

        // Non Prunable due to None as max_timestamp
        let prunable_no_max =
            create_test_bundle(U64::from(10), Some(current_timestamp - 1000), None);
        assert!(!prunable_no_max.is_prunable(U64::from(10), current_timestamp + 1000));

        // Non Prunable due to None as min_timestamp
        let prunable_no_min =
            create_test_bundle(U64::from(10), None, Some(current_timestamp + 1000));
        assert!(!prunable_no_min.is_prunable(U64::from(10), current_timestamp - 1000));

        // The bundle's block number is higher than the given block number and the timestamp is within the valid range
        assert!(!non_prunable.is_prunable(U64::from(5), current_timestamp));

        // Timestamp is within the min and max timestamp range
        assert!(!non_prunable.is_prunable(U64::from(10), current_timestamp + 500));

        // Block number is equal to the bundle's block number and timestamp is within the valid range
        assert!(!non_prunable.is_prunable(U64::from(10), current_timestamp));
    }

    #[test]
    fn test_new_bundle_refund() {
        let refund = BundleRefund::new(None, Some(0), Some(Address::default()));
        assert!(refund.is_none());

        let refund = BundleRefund::new(Some(90), Some(1), Some(Address::default())).expect("valid");
        assert_eq!(refund.percent, 90);
        assert_eq!(refund.index, Some(1));
        assert_eq!(refund.recipient, Some(Address::default()));
    }

    #[test]
    fn test_refund_calc() {
        let refund = BundleRefund::new(Some(80), None, None).expect("valid");
        assert_eq!(
            refund.refund_amount(U256::from(100), U256::from(50), 0),
            U256::from(120)
        );
        assert_eq!(
            refund.refund_amount(U256::from(37000), U256::from(0), 1),
            U256::from(4000)
        );
        assert_eq!(
            refund.refund_amount(U256::from(30000), U256::from(0), 1),
            U256::ZERO
        );
    }
}
