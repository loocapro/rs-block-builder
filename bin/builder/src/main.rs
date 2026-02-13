use builder_primitives::payload::PayloadConfig;
use builder_primitives::run_mode::RunMode;
use bundles::pool::BundlePool;
use clap::Parser;
use cli_ext::{EmptyBuilder, RethBuilderExt};
use consensus_layer::ConsensusLayer;
use payload::service::PayloadBuilderService;
use relay::{internal_relay::MevBoostRelayServer, services::validator::ValidatorScheduleService};
use reth::{cli::Cli, tasks::pool::BlockingTaskGuard};
use reth_node_ethereum::EthereumNode;
use rpc::{BundlesApiExt, BundlesApiServer};
use tracing::{error, log::info};
use tx_network::TxNetwork;

use payload::bidder::policy::{BidSelectionPolicies, DevnetPolicy, LinearSurplusPolicy};
use payload::bidder::service::{BidderService, BidderServiceConfig};
use payload::generator::config::JobGeneratorConfig;

use payload::generator::StreamJobGenerator;

use crate::service::{BlockBuilderService, Config};
use relay::services::relay::RelayService;

pub mod cli_ext;
pub mod service;

fn main() -> eyre::Result<()> {
    Cli::<RethBuilderExt>::parse().run(|builder, args| async move {
        let bundles_pool = BundlePool::new();
        let bundle_pool_two = bundles_pool.clone();

        let cli_args = args.cli_args.clone();
        let cli_args_two = cli_args.clone();
        let handle = builder
            .with_types(EthereumNode::default())
            .with_components(EthereumNode::components().payload(EmptyBuilder::default()))
            .on_node_started(move |ctx| {
                let chain_spec = ctx.chain_spec();

                if cli_args.is_builder_enabled() {
                    // Relay service
                    let (relay_service, relay_handle) = RelayService::new(
                        cli_args.run_mode(),
                        cli_args.is_internal_relay_enabled(),
                        cli_args.bls_public_key(),
                    );
                    ctx.task_executor
                        .clone()
                        .spawn_critical("relay service", Box::pin(relay_service));
                    info!(target: "reth::cli::ext", "Relay service started");

                    // Bidder service
                    let policy = if cli_args.run_mode() == RunMode::Devnet {
                        BidSelectionPolicies::DevnetPolicy(DevnetPolicy)
                    } else {
                        BidSelectionPolicies::LinearSurplusPolicy(LinearSurplusPolicy::new(
                            cli_args.bid_policy_offset(),
                            cli_args.bid_policy_monthly_subsidy_budget(),
                            cli_args.bid_policy_market_share(),
                            cli_args.bid_policy_min_bid_surplus(),
                            cli_args.bid_policy_max_bid_surplus(),
                        ))
                    };

                    let (bidder_service, bidder_handle) = BidderService::new(
                        ctx.provider.clone(),
                        ctx.pool.clone(),
                        ctx.task_executor.clone(),
                        BidderServiceConfig {
                            chain_spec: chain_spec.clone(),
                            policy,
                            relay_handle: relay_handle.clone(),
                            signer: cli_args.bls_signer(),
                        },
                    );
                    ctx.task_executor
                        .clone()
                        .spawn_critical("bidder service", Box::pin(bidder_service));

                    info!(target: "reth::cli::ext", "Bidder service started");

                    // Payload builder service
                    let payload_generator = StreamJobGenerator::new(
                        ctx.provider.clone(),
                        ctx.pool.clone(),
                        bundles_pool.clone(),
                        ctx.task_executor.clone(),
                        JobGeneratorConfig::default().payload_config(PayloadConfig {
                            chain_spec: chain_spec.clone(),
                            builder_address: cli_args.builder_address(),
                            secret_key: *cli_args.ecdsa_secret_key(),
                            extra_data: cli_args.extra_data(),
                        }),
                        bidder_handle.clone(),
                    );
                    let (payload_service, payload_handle) =
                        PayloadBuilderService::new(payload_generator);
                    ctx.task_executor
                        .clone()
                        .spawn_critical("payload-builder service", Box::pin(payload_service));
                    info!(target: "reth::cli::ext", "Payload builder service started");

                    // Consensus layer stream (payload attributes)
                    let payload_attribute_url = format!(
                        "{}eth/v1/events?topics=payload_attributes",
                        cli_args.cl_base_url()
                    );
                    let cl = ConsensusLayer::new(payload_attribute_url);
                    info!(target: "reth::cli::ext", "Consensus layer stream started");

                    // Validator schedule service
                    let (validator_schedule_service, validator_schedule_handle) =
                        ValidatorScheduleService::new(
                            ctx.task_executor.clone(),
                            relay_handle.clone(),
                        );
                    ctx.task_executor.spawn_critical(
                        "validator-schedule service",
                        Box::pin(validator_schedule_service),
                    );

                    // Block builder main service
                    let block_builder_service = BlockBuilderService::new(
                        Config {
                            payload_config: PayloadConfig {
                                chain_spec,
                                builder_address: cli_args.builder_address(),
                                secret_key: *cli_args.ecdsa_secret_key(),
                                extra_data: cli_args.extra_data(),
                            },
                            consensus_layer: cl,
                            payload_handle,
                            bidder_handle,
                            validator_schedule_handle,
                            mev_bundles_pool: bundles_pool.clone(),
                        },
                        ctx.provider,
                    );
                    ctx.task_executor
                        .spawn_critical("block-builder service", Box::pin(block_builder_service));
                    info!(target: "reth::cli::ext", "Block builder service started");
                }
                info!(target: "reth::cli::ext", "Starting custom components...");

                if cli_args.is_tx_network_enabled() {
                    let pool = ctx.pool.clone();
                    let api_key = cli_args.tx_network_api_key().to_string();
                    ctx.task_executor.spawn(Box::pin(async move {
                        if let Err(err) = TxNetwork::run(api_key, pool).await {
                            error!(target: "reth::cli::ext", ?err, "Error from tx network");
                        }
                    }));
                }

                if cli_args.is_internal_relay_enabled() {
                    info!(target: "reth::cli::ext", "Internal relay server enabled");
                    let mev_boost_relay_server =
                        MevBoostRelayServer::spawn(cli_args.internal_relay_exposed_port());
                    ctx.task_executor
                        .spawn_critical("mev-boost-relay server", Box::pin(mev_boost_relay_server));
                }

                Ok(())
            })
            .extend_rpc_modules(move |ctx| {
                if cli_args_two.is_rpc_enabled() {
                    info!(target: "reth::cli::ext", "Extending rpc modules");
                    let eth_api = ctx.registry.eth_api();
                    let pool = ctx.registry.pool().clone();

                    let provider = ctx.provider().clone();
                    let ext = BundlesApiExt {
                        provider,
                        blocking_pool_guard: BlockingTaskGuard::new(
                            cli_args_two.rpc_max_requests(),
                        ),
                        eth_api,
                        pool,
                        bundles_pool: bundle_pool_two.clone(),
                    };
                    ctx.modules.merge_configured(ext.into_rpc())?;
                } else {
                    info!(target: "reth::cli::ext", "Rpc ext not enabled.");
                }

                Ok(())
            })
            .launch()
            .await?;

        handle.wait_for_node_exit().await
    })
}
