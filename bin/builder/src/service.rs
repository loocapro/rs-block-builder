//! Main service that triggers our block builder main loop.
use builder_primitives::payload::{BuildConfig, PayloadConfig};
use bundles::pool::BundlePool;
use consensus_layer::clock::NetworkClock;
use consensus_layer::ConsensusLayer;
use futures_util::StreamExt;
use payload::bidder::service::BidderServiceHandle;
use payload::service::PayloadBuilderHandle;
use relay::services::validator::ValidatorScheduleHandle;
use reth::primitives::{BlockNumberOrTag, SealedBlock, U64};
use reth::providers::{BlockReaderIdExt, BlockSource, StateProviderFactory};
use reth_payload_builder::error::PayloadBuilderError;
use reth_payload_builder::EthPayloadBuilderAttributes;

use std::future::Future;
use std::pin::Pin;

use std::sync::Arc;
use std::task::{Context as StdContext, Poll};
use tracing::{debug, error, info, warn};

pub struct Config {
    pub(crate) payload_config: PayloadConfig,
    /// The consensus layer instance used to listen for payload_attributes
    pub(crate) consensus_layer: ConsensusLayer,
    /// The payload builder handle to send payloads to
    pub(crate) payload_handle: PayloadBuilderHandle,
    /// The bidder service handle to trigger new auctions
    pub(crate) bidder_handle: BidderServiceHandle,
    /// The validator schedule handle to access validator schedule
    pub(crate) validator_schedule_handle: ValidatorScheduleHandle,
    /// Bundle pool to be cleaned after the bid is processed
    pub(crate) mev_bundles_pool: BundlePool,
}

#[must_use = "Block builder does nothing unless polled"]
/// BlockBuilderService is a collection of task handles and is implemented as an endless future
pub struct BlockBuilderService<Client> {
    payload_config: PayloadConfig,
    /// Current slot processed
    slot: u64,
    // Communication
    /// Listen to payload events from the consensus layer and sends them to the payload builder
    consensus_layer: ConsensusLayer,
    /// Handle to comunicate with payload service
    payload_handle: PayloadBuilderHandle,
    /// The bidder service handle to trigger new auctions
    bidder_handle: BidderServiceHandle,
    /// The validator schedule handle to access validator schedule
    validator_schedule_handle: ValidatorScheduleHandle,
    /// The client that can interact with the chain.
    client: Client,
    /// Bundle pool to be cleaned after the bid is processed
    mev_bundles_pool: BundlePool,
}

impl<Client> BlockBuilderService<Client>
where
    Client: StateProviderFactory + BlockReaderIdExt + Clone + Unpin + 'static,
{
    /// Returns a new `BlockBuilderService` given a `ConsensusLayer` and a `PayloadBuilderHandle`.
    pub fn new(configs: Config, client: Client) -> Self {
        let Config {
            payload_config,
            consensus_layer: cl,
            payload_handle,
            validator_schedule_handle,
            mev_bundles_pool,
            bidder_handle,
        } = configs;
        let chain_id = payload_config.chain_spec.chain.id();
        info!(target: "builder::service", chain_id=chain_id, "Spawning block builder service");

        Self {
            payload_config,
            slot: 0,
            consensus_layer: cl,
            payload_handle,
            validator_schedule_handle,
            client,
            mev_bundles_pool,
            bidder_handle,
        }
    }

    /// Retrieves parent block from paylaod_attributes
    pub fn parent_block(
        &self,
        attributes: &EthPayloadBuilderAttributes,
    ) -> Result<SealedBlock, PayloadBuilderError> {
        let parent_block = if attributes.parent.is_zero() {
            // use latest block if parent is zero: genesis block
            self.client
                .block_by_number_or_tag(BlockNumberOrTag::Latest)?
                .ok_or_else(|| PayloadBuilderError::MissingParentBlock(attributes.parent))?
                .seal_slow()
        } else {
            let block = self
                .client
                .find_block_by_hash(attributes.parent, BlockSource::Any)?
                .ok_or_else(|| PayloadBuilderError::MissingParentBlock(attributes.parent))?;

            // we already know the hash, so we can seal it
            block.seal(attributes.parent)
        };

        Ok(parent_block)
    }

    /// Construct the build configs from cl attributes event and slot
    pub fn build_configs(
        &self,
        attributes: EthPayloadBuilderAttributes,
        slot: u64,
    ) -> Option<BuildConfig> {
        // retrieve parent block from payload attributes
        let parent_block = match BlockBuilderService::parent_block(self, &attributes) {
            Ok(block) => Arc::new(block),
            Err(e) => {
                error!(target: "builder::service", error=format!("{:?}", e), "Parent block not found");
                return None;
            }
        };

        let validator_info = match self.validator_schedule_handle.validator_info(slot) {
            Some(info) => info,
            None => {
                warn!(target: "builder::service", payload_id=format!("{:?}", attributes.id), slot=slot, "Validator info not found for slot");
                return None;
            }
        };

        Some(BuildConfig::new(
            &self.payload_config.chain_spec,
            attributes,
            parent_block,
            slot,
            validator_info,
        ))
    }

    pub fn prune_bundle_pool(&mut self, configs: &BuildConfig) {
        let prune_block = U64::from(configs.block_number - 1);
        let prune_timestamp = configs.attributes.timestamp - 12;
        self.mev_bundles_pool.prune(prune_block, prune_timestamp);
        debug!(target: "builder::service", current_block=?configs.block_number, ?prune_block, "Pruned bundle pool");
    }
}

impl<Client> Future for BlockBuilderService<Client>
where
    Client: StateProviderFactory + BlockReaderIdExt + Clone + Unpin + 'static,
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut StdContext<'_>) -> Poll<Self::Output> {
        let mut cl_args: Option<(EthPayloadBuilderAttributes, u64)> = None;
        loop {
            match self.consensus_layer.poll_next_unpin(cx) {
                Poll::Ready(Some((payload, slot))) => {
                    cl_args = Some((payload, slot));
                }
                Poll::Ready(None) => {
                    error!(target: "builder::service", "Payload stream terminated");
                    return Poll::Ready(());
                }
                Poll::Pending => {
                    break;
                }
            }
        }

        if let Some((attributes, slot)) = cl_args {
            // Make sure we only process a single payloadId per event
            if self.slot == slot {
                warn!(target: "builder::service", payload_id=format!("{:?}", attributes.id), slot=slot, "Payload id already known");
                return Poll::Pending;
            }

            // Make sure that payload is processed for current or future slots only
            if attributes.timestamp <= NetworkClock::current_time_as_secs() {
                warn!(target: "builder::service", payload_id=format!("{:?}", attributes.id), slot=slot, "Payload attributes timestamp has past");
                return Poll::Pending;
            }
            info!(?attributes, slot, "new attributes");

            self.slot = slot;
            let build_configs = match self.build_configs(attributes, slot) {
                Some(configs) => configs,
                None => return Poll::Pending,
            };

            self.prune_bundle_pool(&build_configs);

            info!(
                target: "builder::service",
                build_config=build_configs.to_log(),
                "Building new payloads"
            );
            let block_number = build_configs.block_number;
            self.payload_handle.build_new_payload(build_configs);
            self.bidder_handle.on_auction_start(slot, block_number);
        }

        Poll::Pending
    }
}
