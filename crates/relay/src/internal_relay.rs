use builder_primitives::bid::{BidTrace, SignedBid};
use serde::de::DeserializeOwned;
use serde_derive::Deserialize;
use serde_with::{serde_as, DisplayFromStr};
use std::sync::Arc;
use std::{
    collections::{hash_map::Entry, HashMap},
    net::Ipv4Addr,
};
use tokio::sync::Mutex;
use tracing::{debug, error};
use warp::{Filter, Rejection, Reply};

/// Max number of blocks to store bid traces in memory cache
/// Default set to 4 blocks
const MAX_CACHE_BLOCK_SIZE: u64 = 4;

/// In memory cache of bids for the last observed block bid submissions
/// Maps block number to a set of bids received for that particular block
/// Note: only tracks bids submitted by own builder
#[derive(Default)]
pub struct RelayBidCache {
    last_block: u64,
    cache: HashMap<u64, Vec<(SignedBid, BidTrace)>>,
}

impl RelayBidCache {
    /// Add new bid to cache, check size of cache and resize if overflow
    pub fn insert(&mut self, bid: SignedBid) {
        let block_number = bid.block_number();
        let trace = bid.to_trace();
        match self.cache.entry(block_number) {
            Entry::Vacant(entry) => {
                entry.insert(vec![(bid, trace)]);
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().push((bid, trace));
            }
        }
        self.last_block = self.last_block.max(block_number);
        self.resize();
    }

    /// If cache is full, remove block entries that have expired
    pub fn resize(&mut self) {
        if self.last_block < MAX_CACHE_BLOCK_SIZE {
            return;
        }
        let mut expired_blocks: Vec<u64> = self.cache.keys().cloned().collect();
        expired_blocks.retain(|&block| block <= (self.last_block - MAX_CACHE_BLOCK_SIZE));
        for block in expired_blocks {
            self.cache.remove(&block);
        }
    }

    /// Get all bids for a given block number
    pub fn get_all_bids(&self, block_number: u64) -> Option<Vec<(SignedBid, BidTrace)>> {
        self.cache.get(&block_number).map(|bids| bids.to_vec())
    }

    /// Get best bid for a given block number
    pub fn get_best_bid(&self, block_number: u64, timestamp: u64) -> Option<SignedBid> {
        if let Some(bids) = self.get_all_bids(block_number) {
            return bids
                .iter()
                .filter(|(_bid, trace)| trace.timestamp_ms < timestamp)
                .max_by_key(|(bid, _trace)| bid.message.value)
                .cloned()
                .map(|(bid, _trace)| bid);
        }
        None
    }
}

/// Server exposing a restricted relay api
/// Allows block submission by own builder and retrieving bid traces
pub struct MevBoostRelayServer;

mod handlers {
    use super::routes::*;
    use super::*;

    /// Immediately trace SignedBid into BidTrace on receiving and store in cache
    pub async fn submit_block_bid_handler(
        cache: Arc<Mutex<RelayBidCache>>,
        bid: SignedBid,
    ) -> Result<impl Reply, Rejection> {
        let slot = bid.message.slot;
        debug!(target: "relay::internal::server", block=bid.block_number(), ?slot, "Submit block bid request");
        cache.lock().await.insert(bid);
        Ok(warp::reply::with_status(
            "Ok",
            warp::http::status::StatusCode::OK,
        ))
    }

    /// Retrieve bid traces for block number in request
    pub async fn get_bid_traces_handler(
        request: GetBidTracesRequest,
        cache: Arc<Mutex<RelayBidCache>>,
    ) -> Result<impl Reply, Rejection> {
        debug!(target: "relay::internal::server", block=request.block_number, "Get bid traces request");
        match cache.lock().await.get_all_bids(request.block_number) {
            Some(bids) => {
                let traces = bids
                    .into_iter()
                    .map(|(_bid, trace)| trace)
                    .collect::<Vec<BidTrace>>();
                Ok(warp::reply::json::<Vec<BidTrace>>(&traces))
            }
            None => Err(warp::reject::not_found()),
        }
    }

    /// Retrieve best bid for block number in request
    pub async fn get_best_bid_handler(
        request: GetBidRequest,
        cache: Arc<Mutex<RelayBidCache>>,
    ) -> Result<impl Reply, Rejection> {
        debug!(target: "relay::internal::server", block=request.block_number, timestamp=request.timestamp, "Get best bid request");
        match cache
            .lock()
            .await
            .get_best_bid(request.block_number, request.timestamp)
        {
            Some(bid) => Ok(warp::reply::json::<SignedBid>(&bid)),
            None => Err(warp::reject::not_found()),
        }
    }

    /// Generic rejection handler
    pub async fn rejection_handler(err: Rejection) -> Result<impl Reply, Rejection> {
        error!(target: "relay::internal::server", ?err, "Rejection");
        Ok(warp::reply::with_status(
            warp::reply::json(&"internal server error".to_string()),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }
}

mod routes {
    use super::filters::*;
    use super::handlers::*;
    use super::*;
    use builder_primitives::bid::SignedBid;

    /// Submit new bid route
    pub fn submit_block_bid_route(
        cache: Arc<Mutex<RelayBidCache>>,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        warp::path!("relay" / "v1" / "builder" / "blocks")
            .and(warp::post())
            .and(with_cache(cache))
            .and(with_json_body::<SignedBid>())
            .and_then(submit_block_bid_handler)
    }

    /// Request type format to get bid traces and best bid
    /// Note: only supports retreival by block number
    #[serde_as]
    #[derive(Debug, Deserialize)]
    pub struct GetBidTracesRequest {
        #[serde_as(as = "DisplayFromStr")]
        pub block_number: u64,
    }

    // Get bid traces route
    pub fn get_bid_traces_route(
        cache: Arc<Mutex<RelayBidCache>>,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        warp::path!("relay" / "v1" / "data" / "bidtraces" / "builder_blocks_received")
            .and(warp::get())
            .and(warp::query::<GetBidTracesRequest>())
            .and(with_cache(cache))
            .and_then(get_bid_traces_handler)
    }

    /// Request type format to get best bid
    /// Note: only supports retreival by block number and timestamp
    #[serde_as]
    #[derive(Debug, Deserialize)]
    pub struct GetBidRequest {
        #[serde_as(as = "DisplayFromStr")]
        pub block_number: u64,
        #[serde_as(as = "DisplayFromStr")]
        pub timestamp: u64,
    }

    // Get bid traces route
    pub fn get_best_bid_route(
        cache: Arc<Mutex<RelayBidCache>>,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        warp::path!("best_bid")
            .and(warp::get())
            .and(warp::query::<GetBidRequest>())
            .and(with_cache(cache))
            .and_then(get_best_bid_handler)
    }

    pub async fn routes(
        cache: Arc<Mutex<RelayBidCache>>,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        submit_block_bid_route(cache.clone())
            .or(get_bid_traces_route(cache.clone()))
            .or(get_best_bid_route(cache.clone()))
            .recover(handlers::rejection_handler)
    }
}

mod filters {
    use super::*;

    pub fn with_json_body<T: DeserializeOwned + Send>(
    ) -> impl Filter<Extract = (T,), Error = Rejection> + Clone {
        // accept json body and limit payload size to 4mb
        warp::body::content_length_limit(1024 * 1000 * 4).and(warp::body::json())
    }

    /// Pass mutable pointer to cache up webserver call stack
    pub fn with_cache(
        cache: Arc<Mutex<RelayBidCache>>,
    ) -> impl Filter<Extract = (Arc<Mutex<RelayBidCache>>,), Error = std::convert::Infallible> + Clone
    {
        warp::any().map(move || cache.clone())
    }
}

impl MevBoostRelayServer {
    /// Process to serve the relay server
    pub async fn spawn(port: u16) {
        let cache = Arc::new(Mutex::new(RelayBidCache::default()));
        let api = routes::routes(cache).await;
        warp::serve(api).run((Ipv4Addr::UNSPECIFIED, port)).await;
    }
}

#[cfg(test)]
mod tests {
    use builder_primitives::bid::ExecutionPayload;

    use super::*;

    #[test]
    fn test_insert_bid() {
        let mut cache = RelayBidCache::default();
        let bid = SignedBid::default();
        cache.insert(bid.clone());
        assert_eq!(cache.get_all_bids(bid.block_number()).unwrap().len(), 1);
        assert_eq!(cache.last_block, bid.block_number());
    }

    #[test]
    fn test_resize_cache() {
        let mut cache = RelayBidCache::default();

        for i in 0..(MAX_CACHE_BLOCK_SIZE + 10) {
            let bid = SignedBid {
                execution_payload: ExecutionPayload {
                    block_number: i,
                    ..Default::default()
                },
                ..Default::default()
            };
            cache.insert(bid);
        }

        assert!(cache.cache.len() as u64 <= MAX_CACHE_BLOCK_SIZE);
    }
}
