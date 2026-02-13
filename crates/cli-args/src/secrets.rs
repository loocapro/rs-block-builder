use builder_primitives::blst::secret_key::BlsSecretKey;
use clap::Parser;
use secp256k1::SecretKey;

/// Placeholder; set via --builder.bls-secret-key or env. Never use in production.
const DEFAULT_BLS_SECRET_KEY: &str = "REPLACE_ME";
/// Placeholder; set via --builder.ecdsa-secret-key or env. Never use in production.
const DEFAULT_ECDSA_SECRET_KEY: &str = "REPLACE_ME";

#[derive(Debug, Parser, Clone)]
#[clap(next_help_heading = "Secrets")]
pub struct Secrets {
    /// BLS secret key to sign bids (set explicitly; no default for production)
    #[arg(
        long = "builder.bls-secret-key",
        value_parser = parse_bls_secret_key,
        default_value = DEFAULT_BLS_SECRET_KEY
    )]
    pub bls_secret_key: BlsSecretKey,
    /// ECDSA secret key to sign txs (set explicitly; no default for production)
    #[arg(
        long = "builder.ecdsa-secret-key",
        value_parser = parse_ecdsa_secret_key,
        default_value = DEFAULT_ECDSA_SECRET_KEY
    )]
    pub ecdsa_secret_key: SecretKey,
}

/// Helper to parse a [BlsSecretKey] from string
fn parse_bls_secret_key(s: &str) -> Result<BlsSecretKey, String> {
    s.parse::<BlsSecretKey>()
        .map_err(|_| format!("Invalid BLS secret key: {}", s))
}

/// Helper to parse a [SecretKey] from string
fn parse_ecdsa_secret_key(s: &str) -> Result<SecretKey, String> {
    s.parse::<SecretKey>()
        .map_err(|_| format!("Invalid ECDSA secret key: {}", s))
}
