use bidder::BidderConfig;
use builder_primitives::{
    blst::{public_key::BlsPublicKey, secret_key::BlsSecretKey},
    run_mode::RunMode,
    signer::{BlsSigner, ForkVersion},
};
use clap::Parser;
use internal_relay::InternalRelay;
use reth::primitives::{keccak256, Address, Bytes};
use rpc::Rpc;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use secrets::Secrets;
use url::Url;

pub mod bidder;
pub mod internal_relay;
pub mod rpc;
pub mod secrets;
const DEFAULT_CL_URL: &str = "http://127.0.0.1:3500";
const DEFAULT_TX_NETWORK_API_KEY: &str = "";
/// Block header extra_data as hex. Decodes to "merkle Block (bundles.merkle.io)". Use "" for empty.
const DEFAULT_EXTRA_DATA: &str = "6d65726b6c6520426c6f636b202862756e646c65732e6d65726b6c652e696f29";

#[derive(Debug, Parser, Clone)]
#[clap(next_help_heading = "Builder cli args")]
pub struct BaseCliArgs {
    /// CLI flag to enable the builder
    #[clap(long = "builder", default_value_t = false)]
    builder: bool,
    /// CLI flag to enable the tx network
    #[clap(long = "builder.enable-tx-network", default_value_t = false)]
    tx_network_enabled: bool,
    /// Merkle tx network api key
    #[arg(long = "builder.tx_network_api_key", default_value = DEFAULT_TX_NETWORK_API_KEY)]
    tx_network_api_key: String,
    /// Consensus url used to fetch payload attributes
    #[arg(long = "builder.cl-base-url", default_value = DEFAULT_CL_URL)]
    cl_base_url: Url,
    /// Run mode for block builder
    #[arg(long="builder.run-mode", value_parser = parse_run_mode, default_value_t = RunMode::Devnet)]
    run_mode: RunMode,
    /// Configs related to bidding
    #[clap(flatten)]
    bid: BidderConfig,
    /// Rpc configs to enable or specifying a max concurrent requests
    #[clap(flatten)]
    rpc: Rpc,
    /// Secrets configs for ecdsa and bls keys
    #[clap(flatten)]
    secrets: Secrets,
    /// Internal relay configs to enable or disable or specifying a port
    #[clap(flatten)]
    internal_relay: InternalRelay,
    /// Builder extra data field
    #[arg(long = "builder.extra-data", default_value = DEFAULT_EXTRA_DATA)]
    extra_data: String,
}

impl BaseCliArgs {
    /// Get the tx network api key
    pub fn tx_network_api_key(&self) -> &str {
        &self.tx_network_api_key
    }
    /// Get the rpc enabled flag
    pub fn is_rpc_enabled(&self) -> bool {
        self.is_builder_enabled() && self.rpc.rpc_enabled
    }
    /// Get the rpc max concurrent requests
    pub fn rpc_max_requests(&self) -> usize {
        self.rpc.max_requests
    }
    /// Get the base url for the consensus layer
    pub fn cl_base_url(&self) -> &Url {
        &self.cl_base_url
    }
    /// Get the run mode for the block builder
    pub fn run_mode(&self) -> RunMode {
        self.run_mode
    }
    /// Get the max concurrent requests
    pub fn max_requests(&self) -> usize {
        self.rpc.max_requests
    }
    /// Get the bls secret key
    pub fn bls_secret_key(&self) -> &BlsSecretKey {
        &self.secrets.bls_secret_key
    }
    /// Get the bls signer
    pub fn bls_signer(&self) -> BlsSigner {
        let fork_version = if self.run_mode() == RunMode::Devnet {
            ForkVersion::new([0x20, 0x00, 0x00, 0x89])
        } else {
            ForkVersion::default()
        };

        BlsSigner::new(self.bls_secret_key().clone(), fork_version)
    }
    /// Get the bls public key
    pub fn bls_public_key(&self) -> BlsPublicKey {
        self.secrets.bls_secret_key.public_key()
    }
    /// Get the ecdsa secret key
    pub fn ecdsa_secret_key(&self) -> &SecretKey {
        &self.secrets.ecdsa_secret_key
    }
    /// Get the ecdsa public key
    pub fn builder_address(&self) -> Address {
        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, self.ecdsa_secret_key());
        let hash = keccak256(&public_key.serialize_uncompressed()[1..]);
        Address::from_slice(&hash[12..])
    }
    /// Get the internal relay enabled
    pub fn is_internal_relay_enabled(&self) -> bool {
        self.is_builder_enabled() && self.internal_relay.internal_relay
    }

    /// Get the internal relay exposed port
    pub fn internal_relay_exposed_port(&self) -> u16 {
        self.internal_relay.exposed_port
    }

    /// Get the tx network enabled
    pub fn is_tx_network_enabled(&self) -> bool {
        self.is_builder_enabled() && self.tx_network_enabled
    }

    /// Get the builder enabled
    pub fn is_builder_enabled(&self) -> bool {
        self.builder
    }

    /// Get bid policy offset
    pub fn bid_policy_offset(&self) -> i128 {
        self.bid.offset_ms
    }

    /// Get bid policy monthly subsidy budget
    pub fn bid_policy_monthly_subsidy_budget(&self) -> u64 {
        self.bid.monthly_subsidy_budget
    }

    /// Get bid policy market share
    pub fn bid_policy_market_share(&self) -> f64 {
        self.bid.market_share
    }

    /// Get bid policy min bid ssurplus
    pub fn bid_policy_min_bid_surplus(&self) -> u64 {
        self.bid.min_bid_surplus
    }

    /// Get bid policy max bid ssurplus
    pub fn bid_policy_max_bid_surplus(&self) -> u64 {
        self.bid.max_bid_surplus
    }

    /// Get builder extra data
    pub fn extra_data(&self) -> Bytes {
        Bytes::from(hex::decode(self.extra_data.clone()).expect("Bytes valid"))
    }
}

/// Helper to parse a [RunMode] from string
fn parse_run_mode(s: &str) -> Result<RunMode, String> {
    s.parse::<RunMode>()
        .map_err(|_| format!("Invalid run mode: {}", s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Args, Parser};
    use std::str::FromStr;
    use url::Url;
    /// A helper type to parse Args more easily
    #[derive(Parser)]
    struct CommandParser<T: Args> {
        #[clap(flatten)]
        args: T,
    }
    #[test]
    fn parse_configs() {
        let args = CommandParser::<BaseCliArgs>::parse_from([
            "reth",
            "--builder.cl-base-url",
            "http://test_url",
            "--builder.enable-internal-relay",
            "--builder.internal-relay-port",
            "8008",
            "--builder.enable-rpc",
            "--builder.max-rpc-req",
            "100",
            "--builder.bls-secret-key",
            "0x0000000000000000000000000000000000000000000000000000000000000001",
            "--builder.ecdsa-secret-key",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "--builder.run-mode",
            "live",
            "--builder.enable-tx-network",
            "--builder",
            "--builder.bid-offset",
            "2000",
            "--builder.bid-monthly-subsidy-budget",
            "1000000000",
            "--builder.bid-market-share",
            "0.1",
            "--builder.bid-min-bid-surplus",
            "1000000",
            "--builder.bid-max-bid-surplus",
            "2000000",
            "--builder.extra-data",
            "deadbeef",
        ]);

        println!("{:?}", args.args.builder_address());
        assert_eq!(
            args.args.cl_base_url,
            Url::parse("http://test_url").unwrap()
        );
        assert_eq!(args.args.rpc.max_requests, 100);
        assert!(args.args.rpc.rpc_enabled);
        assert_eq!(
            args.args.secrets.bls_secret_key.to_string(),
            "0x0000000000000000000000000000000000000000000000000000000000000001"
        );
        assert_eq!(
            args.args.secrets.ecdsa_secret_key,
            SecretKey::from_str("0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap(),
        );

        assert_eq!(args.args.run_mode, RunMode::Live);
        assert!(args.args.internal_relay.internal_relay);
        assert_eq!(args.args.internal_relay.exposed_port, 8008);
        assert!(args.args.tx_network_enabled);
        assert!(args.args.builder);
        assert_eq!(args.args.bid.offset_ms, 2000);
        assert_eq!(args.args.bid.monthly_subsidy_budget, 1000000000);
        assert_eq!(args.args.bid.market_share, 0.1);
        assert_eq!(args.args.bid.min_bid_surplus, 1000000);
        assert_eq!(args.args.bid.max_bid_surplus, 2000000);
        assert_eq!(args.args.extra_data, "deadbeef");
        assert_eq!(args.args.extra_data().to_vec(), [222, 173, 190, 239]);
    }
}
