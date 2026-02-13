use reth_metrics::{
    metrics::{Counter, Gauge},
    Metrics,
};

/// Payload bidder metrics
#[derive(Metrics, Clone)]
#[metrics(scope = "bundle_pool")]
pub struct BundlePoolMetrics {
    /// Total number of successful bundles added
    pub successful_bundles_added: Counter,
    /// Total number of failed bundles added
    pub failed_bundles_added: Counter,
    /// Total number of successful bundles cancelled
    pub successful_bundles_cancelled: Counter,
    /// Total number of failed bundles cancelled
    pub failed_bundles_cancelled: Counter,
    /// Total number of bundle pool prunes
    pub prunes: Counter,
    /// Number of pruned bundles
    pub pruned_bundles: Counter,
    /// Number of bundles in bundle pool
    pub bundle_pool_size: Gauge,
    /// Number of success send bundle rpc requests
    pub successful_send_bundle_rpc_requests: Counter,
    /// Number of failed send bundle rpc requests
    pub failed_send_bundle_rpc_requests: Counter,
}

impl BundlePoolMetrics {
    /// Incrementing rpc send bundle requests
    pub fn inc_send_bundle_rpc_success(&self) {
        self.successful_send_bundle_rpc_requests.increment(1);
    }
    pub fn inc_send_bundle_rpc_failure(&self) {
        self.failed_send_bundle_rpc_requests.increment(1);
    }
    /// Incrementing bundles added
    pub fn inc_bundles_added(&self, success: bool) {
        if success {
            self.successful_bundles_added.increment(1);
        } else {
            self.failed_bundles_added.increment(1);
        }
    }

    /// Incrementing bundles cancelled
    pub fn inc_bundles_cancelled(&self, success: bool) {
        if success {
            self.successful_bundles_cancelled.increment(1);
        } else {
            self.failed_bundles_cancelled.increment(1);
        }
    }

    /// Incrementing prunes
    pub fn inc_prunes(&self) {
        self.prunes.increment(1);
    }

    /// Incrementing pruned bundles
    pub fn inc_pruned_bundles(&self, bundles: usize) {
        self.pruned_bundles.increment(bundles as u64);
    }

    /// Set bundle pool size
    pub fn set_bundle_pool_size(&self, bundles: usize) {
        self.bundle_pool_size.set(bundles as f64);
    }
}
