use crate::bundle::Bundle;
use crate::inner::BundlePoolInner;
use crate::metrics::BundlePoolMetrics;
use parking_lot::RwLock;
use reth::primitives::B256;
use reth::primitives::U64;
use reth::rpc::eth::error::EthApiError;
use std::sync::Arc;
use std::time::Duration;

const RWLOCK_GAURD_TIMEOUT: u64 = 100;

/// Represents a pool of bundles, indexed by block_number, min_timestamp, max_timestamp and uuid.
#[derive(Debug, Clone)]
pub struct BundlePool {
    inner: Arc<RwLock<BundlePoolInner>>,
    metrics: BundlePoolMetrics,
}

impl Default for BundlePool {
    fn default() -> Self {
        Self::new()
    }
}

impl BundlePool {
    /// Create a new bundle pool
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(BundlePoolInner::new())),
            metrics: BundlePoolMetrics::default(),
        }
    }

    pub fn metrics(&self) -> &BundlePoolMetrics {
        &self.metrics
    }

    /// Add a bundle to the pool
    /// it also index the bundle from the given block number and timestamp
    pub fn add_bundle(&self, bundle: Bundle) -> Option<B256> {
        let hash = self
            .inner
            .try_write_for(Duration::from_millis(RWLOCK_GAURD_TIMEOUT))
            .map(|mut inner| inner.add_bundle(bundle));
        self.metrics.inc_bundles_added(hash.is_some());
        hash
    }

    pub fn len(&self) -> Option<usize> {
        self.inner
            .try_read_for(Duration::from_millis(RWLOCK_GAURD_TIMEOUT))
            .map(|inner| inner.len())
    }

    pub fn is_empty(&self) -> Option<bool> {
        self.inner
            .try_read_for(Duration::from_millis(RWLOCK_GAURD_TIMEOUT))
            .map(|inner| inner.is_empty())
    }

    /// It returns a list of Arc of bundles that are valid for the given block number and timestamp
    /// We use arc to avoid deep cloning the bundles
    pub fn bundles(&self, block_number: U64, timestamp: u64) -> Option<Vec<Arc<Bundle>>> {
        self.inner
            .try_read_for(Duration::from_millis(RWLOCK_GAURD_TIMEOUT))
            .map(|inner| inner.bundles(block_number, timestamp))
    }

    /// It removes a bundle from the pool
    pub fn cancel_bundle(&self, hash: B256) -> Option<Result<(), PoolError>> {
        let result = self
            .inner
            .try_write_for(Duration::from_millis(RWLOCK_GAURD_TIMEOUT))
            .map(|mut inner| inner.cancel_bundle(hash));
        self.metrics.inc_bundles_cancelled(result.is_some());
        result
    }

    /// Prunes old bundles from the pool by block_number and ts
    pub fn prune(&self, block_number: U64, timestamp: u64) {
        let pruned = self
            .inner
            .try_write_for(Duration::from_millis(RWLOCK_GAURD_TIMEOUT))
            .map(|mut inner| inner.prune(block_number, timestamp));
        if let Some((removed, len)) = pruned {
            self.metrics.inc_prunes();
            self.metrics.inc_pruned_bundles(removed);
            self.metrics.set_bundle_pool_size(len);
        }
    }
}
#[derive(Debug)]
pub enum PoolError {
    BundleNotFound,
    BundleReplacementFailed,
}
impl From<PoolError> for EthApiError {
    fn from(e: PoolError) -> Self {
        match e {
            PoolError::BundleNotFound => EthApiError::InvalidParams("bundle not found".to_string()),
            PoolError::BundleReplacementFailed => {
                EthApiError::InvalidParams("bundle replacement failed".to_string())
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use reth::primitives::{B256, U64};
    use uuid::Uuid;

    use crate::bundle::{Bundle, BundleRefund};

    use super::BundlePool;

    fn bundle(
        block_number: U64,
        max_timestamp: Option<u64>,
        min_timestamp: Option<u64>,
        uuid: Option<Uuid>,
    ) -> Bundle {
        Bundle {
            max_timestamp,
            min_timestamp,
            block_number,
            replacement_uuid: uuid,
            hash: B256::random(),
            ..Default::default()
        }
    }

    #[test]
    fn can_replace_bundle() {
        let pool = BundlePool::new();
        let max_ts = Duration::from_secs(10000).as_secs();
        let block_number = U64::from(1);
        let uuid = Uuid::new_v4();
        let first_bundle = bundle(block_number, None, Some(max_ts), Some(uuid));
        let uuid = first_bundle.replacement_uuid.unwrap();
        pool.add_bundle(first_bundle);
        let max_ts = Duration::from_secs(20000).as_secs();
        let block_number = U64::from(10);
        let second_bundle = bundle(block_number, None, Some(max_ts), Some(uuid));
        pool.add_bundle(second_bundle);

        let bundles = pool.bundles(block_number, max_ts).expect("lock acquired");
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].block_number, block_number);
    }

    #[test]
    fn test_bundles() {
        let pool = BundlePool::new();
        let max_ts = Duration::from_secs(10000).as_secs();
        let block_number = U64::from(1);
        let test_bundle = bundle(block_number, Some(max_ts), None, None);

        pool.add_bundle(test_bundle.clone());

        // Test retrieving the bundle
        let bundles = pool.bundles(block_number, max_ts).expect("lock acquired");
        assert_eq!(bundles.len(), 1);
        let result = bundles[0].as_ref().clone();
        assert_eq!(result, test_bundle);

        // Test retrieving with different timestamp
        let bundles = pool
            .bundles(block_number, max_ts + 1)
            .expect("lock acquired");
        assert!(bundles.is_empty());

        // Test retrieving with different block number
        let bundles = pool.bundles(U64::from(2), max_ts).expect("lock acquired");
        assert!(bundles.is_empty());

        // Cleanup: cancel the added bundle
        assert!(pool
            .cancel_bundle(test_bundle.hash)
            .expect("lock acquired")
            .is_ok());

        // Confirm that the bundle is no longer retrievable
        let bundles = pool.bundles(block_number, max_ts).expect("lock acquired");
        assert!(bundles.is_empty());
    }

    #[test]
    fn test_prune() {
        let pool = BundlePool::new();
        let max_ts_1 = Duration::from_secs(10000).as_secs();
        let max_ts_2 = Duration::from_secs(20000).as_secs();
        let block_number_1 = U64::from(1);
        let block_number_2 = U64::from(2);

        // Add two bundles with different max_timestamps and block numbers
        let bundle_1 = bundle(block_number_1, Some(max_ts_1), None, None);
        let bundle_2 = bundle(block_number_2, Some(max_ts_2), None, None);
        pool.add_bundle(bundle_1.clone());
        pool.add_bundle(bundle_2.clone());

        // Prune all bundles lower than block_number_2 or not included in range
        pool.prune(block_number_1, 15000);
        // Check that bundle_1 is removed and bundle_2 is still present
        let bundles = pool
            .bundles(block_number_2, max_ts_2)
            .expect("lock acquired");
        assert_eq!(bundles.len(), 1);
        let result = bundles[0].as_ref().clone();
        assert_eq!(result, bundle_2);

        // Further, ensure that bundle_1 is indeed not present
        let bundles = pool
            .bundles(block_number_1, max_ts_2)
            .expect("lock acquired");
        assert!(bundles.is_empty());
    }
    fn populate_pool_with_varied_timestamps(
        pool: &BundlePool,
        block_count: u64,
        bundles_per_block: usize,
    ) {
        for i in 0..block_count {
            let block_number = U64::from(i + 1);
            for _ in 0..bundles_per_block {
                let uuid = Uuid::new_v4();

                let (min_ts, max_ts) = if i < 90 {
                    (Some(10_000), Some(19_000)) // Will be pruned
                } else {
                    (Some(15_000), Some(25_000)) // Will be pruned
                };

                let bundle = bundle(block_number, max_ts, min_ts, Some(uuid));
                let res = pool.add_bundle(bundle);
                assert!(res.is_some());
            }
        }
    }
    #[test]
    fn test_large_scale_pool_operations() {
        let pool = BundlePool::new();
        let block_count = 100;
        let bundles_per_block = 10;

        populate_pool_with_varied_timestamps(&pool, block_count, bundles_per_block);

        let prune_timestamp = 20_000;
        let prune_block_number = U64::from(90);
        pool.prune(prune_block_number, prune_timestamp);

        let mut remaining_bundles_count = 0;
        for i in 0..block_count {
            let block_number = U64::from(i + 1);
            let bundles = pool
                .bundles(block_number, prune_timestamp)
                .expect("lock acquired");

            remaining_bundles_count += bundles.len();
        }

        let expected_remaining_bundles_count = block_count as usize;
        assert_eq!(remaining_bundles_count, expected_remaining_bundles_count);
    }

    #[test]
    fn test_bundle_with_refund() {
        let pool = BundlePool::new();
        let max_ts = Duration::from_secs(10000).as_secs();
        let block_number = U64::from(1);
        let mut test_bundle = bundle(block_number, Some(max_ts), None, None);
        test_bundle.refund = BundleRefund::new(Some(90), Some(2), None);

        pool.add_bundle(test_bundle.clone());

        // Test retrieving the bundle
        let bundles = pool.bundles(block_number, max_ts).expect("lock acquired");
        assert_eq!(bundles.len(), 1);
        let result = bundles[0].as_ref().clone().refund.expect("valid");
        assert_eq!(result.percent, 90);
        assert_eq!(result.index, Some(2));
        assert_eq!(result.recipient, None);
    }
}
