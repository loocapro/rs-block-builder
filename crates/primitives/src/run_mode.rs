use core::fmt;
use std::str::FromStr;

use serde_with::{DeserializeFromStr, SerializeDisplay};

/// Run mode configuration for block builder
#[derive(Debug, Default, Clone, Copy, PartialEq, SerializeDisplay, DeserializeFromStr, Eq)]
pub enum RunMode {
    /// builds blocks for local devnet
    #[default]
    Devnet,
    /// runs vanilla reth execution client
    Reth,
    /// builds blocks for ETH mainnet but does not submit to mev-boost relays
    Simulate,
    /// builds blocks for ETH mainnet and submits bids to relays
    Live,
}

impl fmt::Display for RunMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunMode::Devnet => f.write_str("devnet"),
            RunMode::Reth => f.write_str("reth"),
            RunMode::Simulate => f.write_str("simulate"),
            RunMode::Live => write!(f, "live"),
        }
    }
}

impl FromStr for RunMode {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "devnet" => Ok(RunMode::Devnet),
            "reth" => Ok(RunMode::Reth),
            "simulate" => Ok(RunMode::Simulate),
            "live" => Ok(RunMode::Live),
            _ => Ok(RunMode::default()),
        }
    }
}
