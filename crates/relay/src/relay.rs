use std::sync::Arc;

use builder_primitives::{
    relays::{RelayInfo, DEVNET, INTERNAL, MERKLE_RELAY},
    run_mode::RunMode,
};
use tracing::info;

use crate::client::{simulate::SimulateRelayClient, Relay, RelayClient};
#[derive(Debug, Clone)]
/// Relay pool holds a list of mev boost relays and is generic to a Relay client.
pub struct RelayPool {
    relays: Vec<RelayInstance>,
}

impl RelayPool {
    pub fn new(_run_mode: RunMode, internal_relay_enabled: bool) -> Self {
        // let mut relays: Vec<RelayInstance> = match run_mode {
        //     RunMode::Devnet => vec![RelayPool::merkle_relay()], // RelayPool::devnet(),
        //     _ => LIVE_RELAYS
        //         .iter()
        //         .map(|r| RelayInstance::new(r, run_mode))
        //         .collect(),
        // };
        let mut relays = vec![RelayPool::merkle_relay()];
        if internal_relay_enabled {
            info!(target: "relay::aggregator", "Internal relay is enabled.");
            relays.push(RelayPool::internal());
        }
        RelayPool { relays }
    }
    pub fn list(&self) -> &Vec<RelayInstance> {
        &self.relays
    }
    pub fn internal() -> RelayInstance {
        RelayInstance::new(&INTERNAL, RunMode::Live)
    }
    pub fn merkle_relay() -> RelayInstance {
        RelayInstance::new(&MERKLE_RELAY, RunMode::Live)
    }
    pub fn devnet() -> RelayInstance {
        RelayInstance::new(&DEVNET, RunMode::Live)
    }
}

/// Relay instance holds relay data and a relay client.
#[derive(Clone, Debug)]
pub struct RelayInstance {
    client: Arc<dyn Relay>,
}

impl RelayInstance {
    pub fn new(relay_info: &RelayInfo, run_mode: RunMode) -> Self {
        let client = Self::create_relay_client(relay_info.clone(), run_mode);
        RelayInstance { client }
    }

    fn create_relay_client(relay_info: RelayInfo, run_mode: RunMode) -> Arc<dyn Relay> {
        match run_mode {
            RunMode::Simulate => Arc::new(SimulateRelayClient::new(relay_info)),
            _ => Arc::new(RelayClient::new(relay_info)),
        }
    }

    pub fn client(&self) -> Arc<dyn Relay> {
        self.client.clone()
    }
}
