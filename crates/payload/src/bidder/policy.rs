use std::{
    ops::{Div, Mul},
    sync::Arc,
};

use builder_primitives::build::MAX_BUILDER_TRANSFER_GAS_LIMIT;
use reth::primitives::{constants::GWEI_TO_WEI, U256};

use crate::strategy::state::{exec_details::ExecutionDetails, BuildState};

const BLOCKS_PER_MONTH: f64 = 200000.0;

/// Converts GWEI values to WEI.
pub fn gwei_to_wei(value: u64) -> U256 {
    U256::from(value) * U256::from(GWEI_TO_WEI)
}

/// Maintained state of parameters necessary to make bid decisions
#[derive(Debug, Default, Clone)]
pub struct AuctionBidInfo {
    slot: u64,
    block_number: u64,
    auction_time_left: i128,
    bid_count: u64,
    best_block: Option<Arc<BuildState>>,
    best_block_value: U256,
    best_bid_value: U256,
    last_bid_value: U256,
    best_auction_bid_value: U256,
    last_auction_bid_value: U256,
    // Additional bid related state fields can go here
}

impl AuctionBidInfo {
    /// Create new bid state for a slot
    pub fn new(slot: u64, block_number: u64) -> Self {
        Self {
            slot,
            block_number,
            auction_time_left: 12000,
            ..Default::default()
        }
    }

    /// Get slot of bid state
    pub fn slot(&self) -> u64 {
        self.slot
    }

    /// Get block number of bid state
    pub fn block_number(&self) -> u64 {
        self.block_number
    }

    /// Get best block
    pub fn best_block(&self) -> Option<Arc<BuildState>> {
        self.best_block.clone()
    }

    ///  Set bid state auction time left
    pub fn refresh_time(&mut self, time_left: i128) {
        self.auction_time_left = time_left;
    }

    /// Update block parameters for a new build state
    pub fn update_block_info(&mut self, build_state: Arc<BuildState>, block_value: U256) {
        if block_value >= self.best_block_value {
            self.best_block = Some(build_state);
            self.best_block_value = block_value;
        }
    }

    /// Update bid parameters for a new bid
    pub fn update_bid_info(&mut self, bid: U256) {
        self.bid_count += 1;
        self.best_bid_value = self.best_bid_value.max(bid);
        self.last_bid_value = bid;
    }

    /// Update auction bid parameters for a new bid
    pub fn update_auction_bid_info(&mut self, bid: U256) {
        self.best_auction_bid_value = self.best_auction_bid_value.max(bid);
        self.last_auction_bid_value = bid;
    }
}

/// Bid selection policy that manages bid calculation
pub trait BidSelectionPolicy {
    /// Calculate block value for this bid policy given execution details
    fn block_value(&self, exec_details: &ExecutionDetails) -> U256;

    /// Check if new execution details is better than current best execution details
    fn is_better_block_value(
        &self,
        best_exec_details: &ExecutionDetails,
        new_exec_details: &ExecutionDetails,
    ) -> bool {
        self.block_value(new_exec_details) > self.block_value(best_exec_details)
    }

    /// Compute bid value for given execution details with current auction bid information
    fn compute_bid(&self, info: &AuctionBidInfo) -> Option<U256>;
}

/// Enum of all bid policies
#[derive(Debug, Clone)]
pub enum BidSelectionPolicies {
    /// Bid with max block value
    NoProfitPolicy(NoProfitPolicy),
    /// Bid with surplus linearly increasing over time in the auction
    LinearSurplusPolicy(LinearSurplusPolicy),
    /// Bid total value on Devnet
    DevnetPolicy(DevnetPolicy),
}

impl BidSelectionPolicy for BidSelectionPolicies {
    fn block_value(&self, exec_details: &ExecutionDetails) -> U256 {
        match self {
            BidSelectionPolicies::NoProfitPolicy(policy) => policy.block_value(exec_details),
            BidSelectionPolicies::LinearSurplusPolicy(policy) => policy.block_value(exec_details),
            BidSelectionPolicies::DevnetPolicy(policy) => policy.block_value(exec_details),
        }
    }

    fn is_better_block_value(
        &self,
        best_exec_details: &ExecutionDetails,
        new_exec_details: &ExecutionDetails,
    ) -> bool {
        match self {
            BidSelectionPolicies::NoProfitPolicy(policy) => {
                policy.is_better_block_value(best_exec_details, new_exec_details)
            }
            BidSelectionPolicies::LinearSurplusPolicy(policy) => {
                policy.is_better_block_value(best_exec_details, new_exec_details)
            }
            BidSelectionPolicies::DevnetPolicy(policy) => {
                policy.is_better_block_value(best_exec_details, new_exec_details)
            }
        }
    }

    fn compute_bid(&self, info: &AuctionBidInfo) -> Option<U256> {
        match self {
            BidSelectionPolicies::NoProfitPolicy(policy) => policy.compute_bid(info),
            BidSelectionPolicies::LinearSurplusPolicy(policy) => policy.compute_bid(info),
            BidSelectionPolicies::DevnetPolicy(policy) => policy.compute_bid(info),
        }
    }
}

/// Policy for bidding with max value from priority fees and coinbase transfers without any net
/// profit
/// Submits bid for every build received immediately after a given offset
#[derive(Debug, Clone)]
pub struct NoProfitPolicy {
    /// time before end of auction to start submitting bids, time is in signed milliseconds
    pub offset: i128,
}

impl BidSelectionPolicy for NoProfitPolicy {
    fn block_value(&self, exec_details: &ExecutionDetails) -> U256 {
        exec_details.sum_fees + exec_details.sum_coinbase_transfers
    }

    fn compute_bid(&self, info: &AuctionBidInfo) -> Option<U256> {
        // check if offset is reached
        let time_left = info.auction_time_left;
        if time_left > self.offset {
            return None;
        }

        if let Some(best_block) = &info.best_block {
            let base_fee = U256::from(best_block.build_config().basefee);
            let builder_payment_gas_limit = U256::from(MAX_BUILDER_TRANSFER_GAS_LIMIT);

            // calculate cost of bid payment transaction
            let bid_transaction_gas_fees = base_fee * builder_payment_gas_limit;

            let total_value = info.best_block_value;

            // max bid is total block value as revenue subtracted by the cost of paying the bid payment
            // as a transfer
            return Some(total_value.saturating_sub(bid_transaction_gas_fees));
        }

        None
    }
}

/// Policy for bidding with priority fees and coinbase transfers as well as a normalized subsidy
/// budget.
/// Bidding strategy follows a time linear weight surplus on top of current auction top bid.
/// Submits bid for every build received immediately after a given offset
#[derive(Debug, Clone)]
pub struct LinearSurplusPolicy {
    /// time before end of auction to start submitting bids, time is in signed milliseconds
    offset: i128,
    /// maximum subsidy allowed per block, computed from monthly budget and target market share
    max_block_subsidy: U256,
    /// minimum surplus to outbid current best bid in auction
    min_bid_surplus: U256,
    /// maximum surplus to outbid current best bid in auction
    max_bid_surplus: U256,
}

impl LinearSurplusPolicy {
    /// Create new policy
    pub fn new(
        offset: i128,
        monthly_subsidy_budget: u64,
        market_share: f64,
        min_bid_surplus: u64,
        max_bid_surplus: u64,
    ) -> Self {
        let target_winning_blocks = (market_share * BLOCKS_PER_MONTH) as u64;
        let max_block_subsidy =
            gwei_to_wei(monthly_subsidy_budget) / U256::from(target_winning_blocks);
        let min_bid_surplus = gwei_to_wei(min_bid_surplus);
        let max_bid_surplus = gwei_to_wei(max_bid_surplus);
        Self {
            offset,
            max_block_subsidy,
            min_bid_surplus,
            max_bid_surplus,
        }
    }
}

impl BidSelectionPolicy for LinearSurplusPolicy {
    fn block_value(&self, exec_details: &ExecutionDetails) -> U256 {
        exec_details.sum_fees + exec_details.sum_coinbase_transfers
    }

    fn compute_bid(&self, info: &AuctionBidInfo) -> Option<U256> {
        // check if offset is reached
        let time_left = info.auction_time_left;
        if time_left > self.offset {
            return None;
        }

        if let Some(best_block) = &info.best_block {
            let base_fee = U256::from(best_block.build_config().basefee);
            let builder_payment_gas_limit = U256::from(MAX_BUILDER_TRANSFER_GAS_LIMIT);

            // calculate cost of bid payment transaction
            let bid_transaction_gas_fees = base_fee * builder_payment_gas_limit;

            // total value is block value subtracted by gas required for payment
            let total_value = info
                .best_block_value
                .saturating_sub(bid_transaction_gas_fees);

            // weight how much of the surplus we want to apply based on linear time weighted amount
            // in the bidding window
            let bid_surplus_diff = self.max_bid_surplus.saturating_sub(self.min_bid_surplus);
            let bid_window = U256::from(self.offset + 1000);
            let time_into_offset = U256::from(self.offset - time_left);
            let to_bid_surplus = bid_surplus_diff
                .mul(time_into_offset)
                .div(bid_window)
                .saturating_add(self.min_bid_surplus);

            // get bid value from the minimum between total value from local block and subsidy and
            // from expected bid from currenct auction top bid and time weighted surplus
            let to_bid_value = info.best_auction_bid_value + to_bid_surplus;
            let max_bid_value = self.max_block_subsidy + total_value;
            let bid_value = max_bid_value.min(to_bid_value);

            if bid_value < info.best_auction_bid_value {
                return None;
            }

            return Some(bid_value);
        }

        None
    }
}

/// Policy for bidding on Devnet, similar to NoProfitPolicy but does not refund builder payment
/// transaction resulting in slightly higher block value
/// Submits bid for every build received immediately
#[derive(Debug, Clone)]
pub struct DevnetPolicy;

impl BidSelectionPolicy for DevnetPolicy {
    fn block_value(&self, exec_details: &ExecutionDetails) -> U256 {
        exec_details.sum_fees + exec_details.sum_coinbase_transfers
    }

    fn compute_bid(&self, info: &AuctionBidInfo) -> Option<U256> {
        let total_value = info.best_block_value;

        // max bid is total block value as revenue
        Some(total_value)
    }
}

#[cfg(test)]
mod tests {

    use builder_primitives::build::test_utils::build_test_config;
    use reth_payload_builder::PayloadId;
    use std::time::Instant;

    use super::*;

    fn test_block_with_fees(fees: u64, coinbase: u64) -> Arc<BuildState> {
        let build_config = build_test_config(PayloadId::new([0u8; 8]));
        let mut block = BuildState::new(build_config, Default::default(), Instant::now());
        block.exec_details_mut().sum_fees = U256::from(fees);
        block.exec_details_mut().sum_coinbase_transfers = U256::from(coinbase);
        Arc::new(block)
    }

    #[test]
    fn test_max_bid_by_total_for_all_builds() {
        let block = test_block_with_fees(1, 1);
        let policy = NoProfitPolicy { offset: 12000 };
        let mut bid_info = AuctionBidInfo::default();

        // test total value
        bid_info.update_block_info(block.clone(), policy.block_value(block.exec_details()));
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(2)));

        // test higher value
        let block = test_block_with_fees(2, 2);
        bid_info.update_block_info(block.clone(), policy.block_value(block.exec_details()));
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(4)));

        // test lower value
        let block = test_block_with_fees(1, 2);
        bid_info.update_block_info(block.clone(), policy.block_value(block.exec_details()));
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(4)));
    }

    #[test]
    fn test_time_linear_by_total_with_subsidy_budget_and_offset_no_subsidy() {
        let block = test_block_with_fees(6, 7);
        let policy = LinearSurplusPolicy {
            offset: 2000,
            max_block_subsidy: U256::from(0),
            min_bid_surplus: U256::from(1),
            max_bid_surplus: U256::from(7),
        };
        let mut bid_info = AuctionBidInfo::default();
        bid_info.update_block_info(block.clone(), policy.block_value(block.exec_details()));

        // test deadline criteria
        bid_info.refresh_time(3000);
        assert_eq!(policy.compute_bid(&bid_info), None);

        // test outbid no auction value
        bid_info.refresh_time(1000);
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(3)));

        // test outbid with auction value
        bid_info.update_auction_bid_info(U256::from(3));
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(6)));

        // test outbid with higher linear weight
        bid_info.refresh_time(0);
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(8)));

        // test outbid with higher linear weight
        bid_info.refresh_time(-500);
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(9)));
    }

    #[test]
    fn test_time_linear_by_total_with_subsidy_budget_and_offset() {
        let block = test_block_with_fees(4, 3);
        let policy = LinearSurplusPolicy {
            offset: 2000,
            max_block_subsidy: U256::from(2),
            min_bid_surplus: U256::from(1),
            max_bid_surplus: U256::from(7),
        };
        let mut bid_info = AuctionBidInfo::default();
        bid_info.update_block_info(block.clone(), policy.block_value(block.exec_details()));

        // test deadline criteria
        bid_info.refresh_time(3000);
        assert_eq!(policy.compute_bid(&bid_info), None);

        // test outbid no auction value
        bid_info.refresh_time(1000);
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(3)));

        // test outbid with auction value
        bid_info.update_auction_bid_info(U256::from(3));
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(6)));

        // test outbid with higher linear weight
        bid_info.refresh_time(0);
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(8)));

        // test outbid with higher linear weight
        bid_info.refresh_time(-500);
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(9)));

        // test outbid with higher linear weight more than max block value
        bid_info.update_auction_bid_info(U256::from(4));
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(9)));

        // test auction greater than max block value
        bid_info.update_auction_bid_info(U256::from(10));
        assert_eq!(policy.compute_bid(&bid_info), None);
    }

    #[test]
    fn test_devnet_bid_for_all_builds() {
        let block = test_block_with_fees(1, 1);
        let policy = DevnetPolicy;
        let mut bid_info = AuctionBidInfo::default();

        // test total value
        bid_info.update_block_info(block.clone(), policy.block_value(block.exec_details()));
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(2)));

        // test higher value
        let block = test_block_with_fees(2, 2);
        bid_info.update_block_info(block.clone(), policy.block_value(block.exec_details()));
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(4)));

        // test lower value
        let block = test_block_with_fees(1, 2);
        bid_info.update_block_info(block.clone(), policy.block_value(block.exec_details()));
        assert_eq!(policy.compute_bid(&bid_info), Some(U256::from(4)));
    }
}
