use clap::Parser;

/// The default maximum number of concurrently executed requests.
pub const DEFAULT_MAX_RPC_REQUESTS: usize = 25;

#[derive(Debug, Parser, Clone)]
#[clap(next_help_heading = "BuilderRpc")]
pub struct Rpc {
    /// CLI flag to enable the validation extension namespace
    #[clap(long = "builder.enable-rpc", default_value_t = false)]
    pub rpc_enabled: bool,
    /// Maximum number of concurrent bundle api requests.
    #[arg(long="builder.max-rpc-req", value_name = "COUNT", default_value_t = DEFAULT_MAX_RPC_REQUESTS)]
    pub max_requests: usize,
}
