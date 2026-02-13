use std::time::Duration;

use reth_metrics::{
    metrics::{Counter, Histogram},
    Metrics,
};

/// Payload bidder metrics
#[derive(Metrics, Clone)]
#[metrics(scope = "payloads")]
pub struct PayloadBidderMetrics {
    /// Total number of initiated build state finalization tasks
    pub initiated_finalized_builds: Counter,
    /// Total number of successful build state finalization tasks
    pub successful_finalized_builds: Counter,
    /// Total number of failed build state finalization tasks
    pub failed_finalized_builds: Counter,
    /// Build state finalization latency
    pub finalize_build_latency: Histogram,
    /// Start to build state finalization latency
    pub to_finalize_build_latency: Histogram,
    /// Total number of initiated bid attempts
    pub initiated_bids: Counter,
    /// Total number of successful bid attempts
    pub successful_bids: Counter,
    /// Total number of failed bid attempts
    pub failed_bids: Counter,
    /// Bid latency
    pub bid_latency: Histogram,
    /// Start to bid latency
    pub to_bid_latency: Histogram,
    /// Total number of auctions
    pub auction_infos: Counter,
    /// Total number of auction top bids received
    pub auction_top_bids: Counter,
}

impl PayloadBidderMetrics {
    /// Incrementing initiated finalized builds
    pub fn inc_initiated_finalized_builds(&self) {
        self.initiated_finalized_builds.increment(1);
    }

    /// Incrementing successful finalized builds
    pub fn inc_successful_finalized_builds(&self) {
        self.successful_finalized_builds.increment(1);
    }

    /// Incrementing failed finalized builds
    pub fn inc_failed_finalized_builds(&self) {
        self.failed_finalized_builds.increment(1);
    }

    /// Recording finalized build latency
    pub fn record_finalize_build_latency(&self, elapsed: Duration) {
        self.finalize_build_latency.record(elapsed);
    }

    /// Recording complete start to finalize build latency
    pub fn record_to_finalize_build_latency(&self, elapsed: Duration) {
        self.to_finalize_build_latency.record(elapsed);
    }

    /// Incrementing initiated bids
    pub fn inc_initiated_bids(&self) {
        self.initiated_bids.increment(1);
    }

    /// Incrementing successful bids
    pub fn inc_successful_bids(&self) {
        self.successful_bids.increment(1);
    }

    /// Incrementing failed bids
    pub fn inc_failed_bids(&self) {
        self.failed_bids.increment(1);
    }

    /// Recording bid latency
    pub fn record_bid_latency(&self, elapsed: Duration) {
        self.bid_latency.record(elapsed);
    }

    /// Recording complete start to bid latency
    pub fn record_to_bid_latency(&self, elapsed: Duration) {
        self.to_bid_latency.record(elapsed);
    }

    /// Incrementing unique auction infos received
    pub fn inc_auction_infos(&self) {
        self.auction_infos.increment(1);
    }

    /// Incrementing auction top bids received
    pub fn inc_auction_top_bids(&self) {
        self.auction_top_bids.increment(1);
    }
}
