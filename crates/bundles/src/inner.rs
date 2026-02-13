use reth::primitives::B256;
use reth::primitives::U64;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tracing::debug;
use tracing::error;
use uuid::Uuid;

use crate::bundle::Bundle;
use crate::pool::PoolError;

/// A generic index structure that maps keys of type `K` to lists of values of type `V`.
///
/// `Index` is a wrapper around `BTreeMap`, providing a convenient way to associate
/// each key with multiple values. It is designed to offer efficient lookups,
/// insertions, and deletions.
#[derive(Debug)]
pub(crate) struct Index<K, V>(BTreeMap<K, Vec<V>>);

impl<K, V> AsRef<BTreeMap<K, Vec<V>>> for Index<K, V> {
    fn as_ref(&self) -> &BTreeMap<K, Vec<V>> {
        &self.0
    }
}

impl<K, V> AsMut<BTreeMap<K, Vec<V>>> for Index<K, V> {
    fn as_mut(&mut self) -> &mut BTreeMap<K, Vec<V>> {
        &mut self.0
    }
}

impl<K, V> Index<K, V>
where
    K: Ord + Clone,
    V: Clone,
{
    pub(crate) fn new() -> Self {
        Self(BTreeMap::new())
    }

    fn insert(&mut self, key: K, value: V) {
        self.0.entry(key).or_default().push(value);
    }
}

pub(crate) type BundlesMap = HashMap<B256, Arc<Bundle>>;
pub(crate) type BlockNumberIndex = Index<U64, B256>;
pub(crate) type TimestampIndex = Index<u64, B256>;
pub(crate) type UuidIndex = Index<Uuid, B256>;

#[derive(Debug)]
pub(crate) struct BundlePoolInner {
    bundles: BundlesMap,
    block_number_index: BlockNumberIndex,
    min_timestamp_index: TimestampIndex,
    max_timestamp_index: TimestampIndex,
    uuid_index: UuidIndex,
}

impl BundlePoolInner {
    pub(crate) fn new() -> Self {
        debug!(target: "bundle_pool", "Bundle pool initialized");
        Self {
            bundles: HashMap::new(),
            block_number_index: Index::new(),
            min_timestamp_index: Index::new(),
            max_timestamp_index: Index::new(),
            uuid_index: Index::new(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.bundles.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.bundles.is_empty()
    }

    /// Adds a bundle to the pool
    pub(crate) fn add_bundle(&mut self, bundle: Bundle) -> B256 {
        let Bundle {
            hash,
            block_number,
            min_timestamp,
            max_timestamp,
            replacement_uuid,
            ..
        } = bundle;

        // If there is a bundle with the same uuid, cancel it
        // other wise index the uuid
        if replacement_uuid
            .map(|uuid| self.handle_replacement_uuid(uuid, hash).is_err())
            .unwrap_or(false)
        {
            error!(target: "bundle_pool", "Could not replace bundle, not found {}", hash);
        }
        self.insert_bundle(bundle);
        self.index_block_number(block_number, hash);

        if let Some(timestamp) = min_timestamp {
            self.index_min_ts(timestamp, hash);
        }

        if let Some(timestamp) = max_timestamp {
            self.index_max_ts(timestamp, hash);
        }
        debug!(target: "bundle_pool", "New bundle added {}", hash);
        hash
    }

    /// It returns a list of bundles that are valid for the given block number and timestamp
    pub(crate) fn bundles(&self, block_number: U64, timestamp: u64) -> Vec<Arc<Bundle>> {
        let bundles: Vec<Arc<Bundle>> = self
            .block_number_index
            .as_ref()
            .get(&block_number)
            .iter()
            .flat_map(|hashes| {
                hashes
                    .iter()
                    .filter_map(|hash| self.get_by_block_and_ts(hash, block_number, timestamp))
            })
            .collect();

        bundles
    }
    /// Retrieves a bundle if it is valid for the given block number and timestamp.
    fn get_by_block_and_ts(
        &self,
        hash: &B256,
        block_number: U64,
        timestamp: u64,
    ) -> Option<Arc<Bundle>> {
        let bundle = self
            .bundles
            .get(hash)
            .filter(|bundle| {
                bundle.is_within_timestamp(timestamp) && bundle.is_within_block_number(block_number)
            })
            .cloned();
        bundle
    }

    /// Cancels and removes a bundle from the pool.
    pub(crate) fn cancel_bundle(&mut self, hash: B256) -> Result<(), PoolError> {
        if let Some(bundle) = self.remove_bundle(&hash) {
            if let Some(key) = bundle.min_timestamp {
                if let Some(values) = self.min_timestamp_index.as_mut().get_mut(&key) {
                    values.retain(|v| v != &hash);
                }
            }

            if let Some(key) = Some(bundle.block_number) {
                if let Some(values) = self.block_number_index.as_mut().get_mut(&key) {
                    values.retain(|v| v != &hash);
                }
            }

            debug!(target: "bundle_pool", "Cancelled bundle {}", hash);
            return Ok(());
        }
        error!(target: "bundle_pool", "Could not delete bundle, not found {}", hash);
        Err(PoolError::BundleNotFound)
    }

    /// Prunes the pool by removing bundles that are no longer valid.
    /// A bundle is no longer valid if it is older than the given block number or
    /// the timestamp is not included in max and min timestamps.
    ///
    /// If max and min timestamps are not set, the bundle is valid for any timestamp
    pub(crate) fn prune(&mut self, block_number: U64, timestamp: u64) -> (usize, usize) {
        let mut removed_hashes = Vec::new();

        self.bundles.retain(|hash, bundle| {
            if bundle.is_prunable(block_number, timestamp) {
                removed_hashes.push(*hash);
                false
            } else {
                true
            }
        });

        for (_, values) in self.block_number_index.as_mut().iter_mut() {
            values.retain(|hash| !removed_hashes.contains(hash));
        }
        for (_, values) in self.min_timestamp_index.as_mut().iter_mut() {
            values.retain(|hash| !removed_hashes.contains(hash));
        }
        for (_, values) in self.max_timestamp_index.as_mut().iter_mut() {
            values.retain(|hash| !removed_hashes.contains(hash));
        }
        for (_, values) in self.uuid_index.as_mut().iter_mut() {
            values.retain(|hash| !removed_hashes.contains(hash));
        }

        let removed = removed_hashes.len();
        let size = self.bundles.len();

        debug!(
            target: "bundle_pool",
            "Pruned {} bundles for block_number: {}, timestamp {}, bundles left: {}",
            removed,
            block_number,
            timestamp,
            size,
        );
        (removed, size)
    }

    /// Inserts a new bundle into the pool.
    fn insert_bundle(&mut self, bundle: Bundle) {
        self.bundles.insert(bundle.hash, Arc::new(bundle));
    }

    /// Inserts a hash into the block number index.
    fn index_block_number(&mut self, block_number: U64, hash: B256) {
        self.block_number_index.insert(block_number, hash);
    }

    /// Inserts a hash into the minimum timestamp index.
    fn index_min_ts(&mut self, timestamp: u64, hash: B256) {
        self.min_timestamp_index.insert(timestamp, hash);
    }

    /// Inserts a hash into the maximum timestamp index.
    fn index_max_ts(&mut self, timestamp: u64, hash: B256) {
        self.max_timestamp_index.insert(timestamp, hash);
    }

    /// Removes a bundle from the pool and returns it if it existed.
    fn remove_bundle(&mut self, hash: &B256) -> Option<Arc<Bundle>> {
        self.bundles.remove(hash)
    }

    /// Find a bundle by uuid
    fn find_by_uuid(&self, uuid: Uuid) -> Option<B256> {
        let hash = self
            .uuid_index
            .as_ref()
            .get(&uuid)
            .and_then(|hashes| hashes.first().copied());
        hash
    }

    /// If a bundle with the given uuid exists, it cancels it
    /// otherwise it indexes the uuid
    fn handle_replacement_uuid(&mut self, uuid: Uuid, hash: B256) -> Result<(), PoolError> {
        if let Some(hash) = self.find_by_uuid(uuid) {
            self.cancel_bundle(hash)
                .map_err(|_| PoolError::BundleReplacementFailed)
        } else {
            self.uuid_index.insert(uuid, hash);
            Ok(())
        }
    }
}
