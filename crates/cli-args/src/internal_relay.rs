use clap::Parser;

const DEFAULT_INTERNAL_RELAY_PORT: u16 = 8008;
const DEFAULT_ENABLE_INTERNAL_RELAY_SERVER: bool = false;

#[derive(Debug, Parser, Clone)]
#[clap(next_help_heading = "Internal Relay")]
pub struct InternalRelay {
    /// Run MevBoostRelayServer if true
    /// Only used to cache bids and expose relay data api for metrics used by
    /// monitor application
    #[arg(long = "builder.enable-internal-relay", default_value_t = DEFAULT_ENABLE_INTERNAL_RELAY_SERVER)]
    pub internal_relay: bool,
    /// Port for internal relay server
    #[arg(long = "builder.internal-relay-port", default_value_t = DEFAULT_INTERNAL_RELAY_PORT)]
    pub exposed_port: u16,
}
