use cli_args::BaseCliArgs;

use payload::generator::empty::EmptyBlockPayloadJobGenerator;

use reth::builder::components::PayloadServiceBuilder;
use reth::builder::{BuilderContext, FullNodeTypes};
use reth::cli::config::PayloadBuilderConfig;

use reth::providers::CanonStateSubscriptions;

use reth::transaction_pool::TransactionPool;
use reth_basic_payload_builder::BasicPayloadJobGeneratorConfig as RethJobGeneratorConfig;

use reth_node_ethereum::EthEngineTypes;

use reth_payload_builder::PayloadBuilderService as RethPayloadBuilderService;
use tracing::info;

#[derive(Debug, Clone, clap::Args)]
pub struct RethBuilderExt {
    #[clap(flatten)]
    pub cli_args: BaseCliArgs,
}

/// An empty payload builder used to build default blocks, this will be used by the CL engine api on DEVNET mode since we will be validating payloads
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct EmptyBuilder;

impl<Node, Pool> PayloadServiceBuilder<Node, Pool> for EmptyBuilder
where
    Node: FullNodeTypes<Engine = EthEngineTypes>,
    Pool: TransactionPool + Unpin + 'static,
{
    async fn spawn_payload_service(
        self,
        ctx: &BuilderContext<Node>,
        pool: Pool,
    ) -> eyre::Result<reth_payload_builder::PayloadBuilderHandle<Node::Engine>> {
        info!(target: "reth::cli::ext", "Spawning reth builder");

        let chain_spec = ctx.chain_spec();
        let conf = ctx.payload_builder_config();

        let payload_builder = reth_ethereum_payload_builder::EthereumPayloadBuilder::default();
        // Using empty payload job generator that will be used by the CL engine api on DEVNET mode since we will be validating payloads
        let payload_generator = EmptyBlockPayloadJobGenerator::with_builder(
            ctx.provider().clone(),
            pool,
            ctx.task_executor().clone(),
            RethJobGeneratorConfig::default()
                .interval(conf.interval())
                .deadline(conf.deadline())
                .max_payload_tasks(conf.max_payload_tasks())
                .extradata(conf.extradata_bytes())
                .max_gas_limit(conf.max_gas_limit()),
            chain_spec.clone(),
            payload_builder,
        );

        let (payload_service, payload_handle) = RethPayloadBuilderService::new(
            payload_generator,
            ctx.provider().canonical_state_stream(),
        );
        ctx.task_executor()
            .spawn_critical("reth payload-builder service", Box::pin(payload_service));
        info!(target: "reth::cli::ext", "Reth default payload builder service started");
        // returning a payload handle to an empty payload job generator so that we can build payloads remotely
        // the CL will always use local payloads if they have greater or equal block value than a remote payaload
        Ok(payload_handle)
    }
}
