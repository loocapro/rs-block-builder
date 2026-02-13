use std::sync::Arc;

use crate::{
    client::RelayClientErr,
    relay::{RelayInstance, RelayPool},
};

use builder_primitives::{
    bid::{RelayBidTrace, SignedBid},
    validator::{ValidatorSchedule, ValidatorScheduleSlotInfo},
};

use futures_util::{stream::FuturesUnordered, Future, StreamExt};
use reqwest::StatusCode;
use tracing::{debug, error, info, trace};

/// A structure to aggregate and manage interactions with a pool of relay instances.
///
/// `RelayAggregator` provides high-level methods to interact with a collection of relay instances,
/// allowing for operations like gathering validator data, broadcasting bids, and retrieving bid traces.
///
/// # Fields
/// - `relays`: A `RelayPool` instance containing the pool of relay instances.
#[derive(Debug, Clone)]
pub struct RelayAggregator {
    relays: RelayPool,
}

impl RelayAggregator {
    /// Creates a new `RelayAggregator` instance.
    ///
    /// # Arguments
    /// - `relays`: The `RelayPool` instance containing the relay instances to be aggregated.
    ///
    /// # Returns
    /// Returns a new instance of `RelayAggregator`.
    pub fn new(relays: RelayPool) -> Self {
        Self { relays }
    }

    /// Fetches validator schedule from all relay instances.
    ///
    /// This method aggregates validator schedules from each relay in the pool
    /// If no valid data is obtained from any relay, it returns an error.
    ///
    /// # Returns
    /// Returns `Ok(ValidatorSchedule)` if successful, or an error of type `RelayClientErr` if no data is obtained.
    pub async fn load_validator_schedule(&self) -> Result<ValidatorSchedule, RelayClientErr> {
        let results = self
            .aggregate(|relay| {
                let client = relay.client();
                let info = relay.client().info();
                info!(?info, "Loading validators from");
                async move { client.get_validators().await }
            })
            .await;

        let v_info: Vec<ValidatorScheduleSlotInfo> = results
            .into_iter()
            .flatten()
            .map(ValidatorScheduleSlotInfo::from)
            .collect();

        if v_info.is_empty() {
            error!(target: "relay::aggregator", "Failed to get validators from all relays");
            return Err(RelayClientErr::EmptyValidatorRegistrations);
        }

        let schedule = ValidatorSchedule::new(v_info);
        trace!(
            target: "relay::aggregator",
            "Loaded validators regs for {} slots",
            schedule.as_ref().len()
        );
        Ok(schedule)
    }

    /// Broadcasts a signed bid to all relay instances in the pool.
    ///
    /// This method sends a signed bid to each relay instance.
    /// It logs the success or failure of these attempts and returns an error if all attempts fail.
    ///
    /// # Arguments
    /// - `signed_bid`: An `Arc<SignedBid>` representing the bid to be broadcasted.
    ///
    /// # Returns
    /// Returns `Ok(())` if the bid is successfully broadcasted to at least one relay,
    /// or an error of type `RelayClientErr` if all broadcasts fail.
    pub async fn broadcast_submit_bid(
        &self,
        signed_bid: Arc<SignedBid>,
    ) -> Result<(), RelayClientErr> {
        let slot = signed_bid.message.slot;
        let block = signed_bid.execution_payload.block_number;
        trace!(target: "relay::aggregator", ?signed_bid, "Broadcasting signed bid");

        let broadcast_fn = |relay: &RelayInstance| {
            let bid_clone = signed_bid.clone();
            let client = relay.client();

            let name = client.info().name().clone();
            async move {
                let resp = client.submit_bid(bid_clone).await.map_err(|err| {
                    error!(
                        target: "relay::client",
                        ?slot,
                        ?block,
                        relay=name.to_string(),
                        ?err,
                        "Failed to send bid submission",
                    );
                    err
                })?;
                if resp.status != StatusCode::OK {
                    let err = resp.message;
                    error!(
                        target: "relay::client",
                        relay=name.to_string(),
                        ?slot,
                        ?block,
                        ?err,
                        "Failed bid submission",
                    );
                    return Err(RelayClientErr::Submit(err));
                }
                debug!(target: "relay::aggregator", ?slot, ?block, relay=name.to_string(), "Submitted bid");
                Ok(())
            }
        };
        let results = self.aggregate(broadcast_fn).await;

        let successful_submissions = results.iter().filter(|result| result.is_ok()).count();

        match successful_submissions {
            0 => {
                error!(target: "relay::aggregator",?slot, ?block, "Failed to broadcast signed bid to relays");
                Err(RelayClientErr::Broadcast)
            }
            _ => {
                debug!(target: "relay::aggregator",?slot, ?block, "Broadcasted signed bid to {} relays", successful_submissions);
                Ok(())
            }
        }
    }

    /// Retrieves bid traces for a specified block number from all relay instances.
    ///
    /// This method collects bid traces associated with a specific block number from each relay in the pool.
    ///
    /// # Arguments
    /// - `block_number`: The block number for which bid traces are required.
    ///
    /// # Returns
    /// Returns a `Vec<RelayBidTrace>` containing all the bid traces for the specified block number.
    pub async fn bid_traces(&self, block_number: u64) -> Vec<RelayBidTrace> {
        let results = self
            .aggregate(|relay| {
                let client = relay.client();
                async move { client.get_bid_traces(block_number).await }
            })
            .await;

        results.into_iter().flatten().collect()
    }

    /// Fetches the best bid for a given block number from the internal relay pool.
    ///
    /// This method queries the internal relay for the best bid associated with a specific block number.
    ///
    /// # Arguments
    /// - `block_number`: The block number for which the best bid is required.
    /// - `timestamp`: The higher limit on valid timestamp to filter late bids
    ///
    /// # Returns
    /// Returns an `Option<SignedBid>` with the best bid for the specified block number, or `None` if not available.
    pub async fn internal_best_bid(&self, block_number: u64, timestamp: u64) -> Option<SignedBid> {
        let internal = RelayPool::internal();
        internal
            .client()
            .get_internal_best_bid(block_number, timestamp)
            .await
    }

    /// Aggregates the results of applying a provided asynchronous function to each `RelayInstance`.
    ///
    /// This function takes a closure `f` and applies it to each `RelayInstance` in the `relays` collection.
    /// The closure `f` should be an asynchronous function that returns a `Future`, which upon completion,
    /// yields a result of type `R`. The function then collects all these results into a `Vec<R>`.
    async fn aggregate<F, Fut, R>(&self, mut f: F) -> Vec<R>
    where
        F: FnMut(&RelayInstance) -> Fut,
        Fut: Future<Output = R>,
    {
        let mut futures = FuturesUnordered::new();

        for relay in self.relays.list() {
            let future = f(relay);
            futures.push(future);
        }

        let mut results = Vec::new();
        while let Some(result) = futures.next().await {
            results.push(result);
        }

        results
    }
}
