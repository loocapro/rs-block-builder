use reth_metrics::{metrics::Counter, Metrics};

/// Payload builder service metrics
#[derive(Metrics)]
#[metrics(scope = "ofac_metrics")]
pub(crate) struct OfacMetrics {
    /// Total number of censored addresses
    pub(crate) censored: Counter,
}

impl OfacMetrics {
    // Function to increment the censored counter.
    pub(crate) fn increment_censored(&self, n: u64) {
        self.censored.increment(n);
    }
}
