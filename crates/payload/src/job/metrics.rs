use std::time::Duration;

use reth_metrics::{
    metrics::{Counter, Histogram},
    Metrics,
};

/// Payload builder metrics
#[derive(Metrics)]
#[metrics(scope = "payloads")]
pub struct PayloadBuilderMetrics {
    /// Total number of initiated payload build attempts
    pub initiated_payload_builds: Counter,
    /// Total number of successful payload build attempts
    pub successful_payload_builds: Counter,
    /// Total number of failed payload build attempts
    pub failed_payload_builds: Counter,
    /// Build latency
    pub payload_build_latency: Histogram,
}

impl PayloadBuilderMetrics {
    /// Incrementing initiated payload builds
    pub fn inc_initiated_payload_builds(&self) {
        self.initiated_payload_builds.increment(1);
    }

    /// Incrementing successful payload builds
    pub fn inc_successful_payload_builds(&self) {
        self.successful_payload_builds.increment(1);
    }

    /// Incrementing failed payload builds
    pub fn inc_failed_payload_builds(&self) {
        self.failed_payload_builds.increment(1);
    }

    /// Recording record of build latency
    pub fn record_payload_build_latency(&self, elapsed: Duration) {
        self.payload_build_latency.record(elapsed);
    }
}
