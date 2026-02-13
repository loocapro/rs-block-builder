use clap::Parser;

const DEFAULT_BID_OFFSET: i128 = 1000;
const DEFAULT_BID_MONTHLY_SUBSIDY_BUDGET: u64 = 0;
const DEFAULT_BID_MARKET_SHARE: f64 = 1.0;
const DEFAULT_BID_MIN_BID_SURPLUS: u64 = 0;
const DEFAULT_BID_MAX_BID_SURPLUS: u64 = 0;

#[derive(Debug, Parser, Clone)]
#[clap(next_help_heading = "Bidder")]
pub struct BidderConfig {
    /// Bidder auction deadline offset, time from end of auction when the bidder starts submitting
    /// bids, default is 1000 ms
    #[clap(long = "builder.bid-offset", default_value_t = DEFAULT_BID_OFFSET)]
    pub offset_ms: i128,
    /// Bidder monthly subsidy budget in gwei, amount willing to pay per month in subsidies,
    /// default is 0 gwei
    #[clap(long = "builder.bid-monthly-subsidy-budget", default_value_t = DEFAULT_BID_MONTHLY_SUBSIDY_BUDGET)]
    pub monthly_subsidy_budget: u64,
    /// Bidder market share, target percent of blocks won with subsidy budget, this argument helps
    /// determine a per block subsidy amount, default is 1.0 = 100%
    #[clap(long = "builder.bid-market-share", default_value_t = DEFAULT_BID_MARKET_SHARE)]
    pub market_share: f64,
    /// Bidder minimum bid surplus in gwei, minimum amount to outbid the current best bid in the
    /// auction, default is 0 gwei
    #[clap(long = "builder.bid-min-bid-surplus", default_value_t = DEFAULT_BID_MIN_BID_SURPLUS)]
    pub min_bid_surplus: u64,
    /// Bidder maximum bid surplus in gwei, maximum amount to outbid the current best bid in the
    /// auction, default is 0 gwei
    #[clap(long = "builder.bid-max-bid-surplus", default_value_t = DEFAULT_BID_MAX_BID_SURPLUS)]
    pub max_bid_surplus: u64,
}
