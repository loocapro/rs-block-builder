use reth::primitives::serde_helper::num::u64_hex_or_decimal;
use reth::primitives::{Chain, NamedChain};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::time::Duration;
use tokio::time::Sleep;

pub const MAINNET_GENESIS_TS: u64 = 1606824023;
pub const HOLESKY_GENESIS_TS: u64 = 1695902100;
pub const SECONDS_PER_SLOT: u64 = 12;

#[derive(Debug, Error)]
pub enum NetworkClockErr {
    #[error("Unsupported chain: {0}")]
    UnsupportedChain(String),
    #[error("Could not read devnet genesis file")]
    DevnetGenesis(#[from] serde_json::Error),
}

/// Network clock utility struct
#[derive(Debug)]
pub struct NetworkClock {
    genesis_ts: u128,
    seconds_per_slot: u128,
}

impl NetworkClock {
    fn new(genesis_ts: u64, seconds_per_slot: u64) -> Self {
        Self {
            genesis_ts: Duration::from_secs(genesis_ts).as_nanos(),
            seconds_per_slot: Duration::from_secs(seconds_per_slot).as_nanos(),
        }
    }

    /// Get current unix time in nanoseconds
    pub fn current_time() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Should never go back in time")
            .as_nanos()
    }

    /// Get current unix time in seconds
    pub fn current_time_as_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Should never go back in time")
            .as_secs()
    }

    /// Converts `timestamp` with **nanosecond** precision to a `Slot`.
    /// Returns `None` if `timestamp` is before the `genesis_time`.
    pub fn current_slot(&self) -> Option<u64> {
        let current_time = NetworkClock::current_time();
        let since_genesis = current_time.checked_sub(self.genesis_ts)?;
        Some(u64::try_from(since_genesis / self.seconds_per_slot).expect("overflow error"))
    }

    /// Returns the timestamp in nanos of a given slot
    pub fn slot_timestamp_nanos(&self, slot: u64) -> u128 {
        u128::from(slot) * self.seconds_per_slot + self.genesis_ts
    }

    /// Returns the timestamp of a given slot
    pub fn slot_timestamp(&self, slot: u64) -> u64 {
        let slot_time_in_nanos = self.slot_timestamp_nanos(slot);
        (slot_time_in_nanos / 1_000_000_000) as u64 // Convert nanoseconds to seconds
    }

    /// Returns the duration until the given slot
    pub fn time_until_slot(&self, slot: u64) -> Duration {
        let current_time = NetworkClock::current_time();
        let target_slot_in_nanos = self.slot_timestamp_nanos(slot);

        target_slot_in_nanos
            .checked_sub(current_time)
            .map(|t| Duration::from_nanos(t.try_into().expect("overflow error")))
            .unwrap_or_default()
    }

    /// Returns the signed milliseconds delta until slot
    pub fn time_until_slot_signed_ms(&self, slot: u64) -> i128 {
        let current_time = NetworkClock::current_time();
        let target_slot_in_nanos = self.slot_timestamp_nanos(slot);

        (target_slot_in_nanos as i128 - current_time as i128) / 1000000
    }

    /// Returns a future that will resolve before the deadline of the target slot
    pub fn slot_deadline(&self, target_slot: u64, deadline: Duration) -> Pin<Box<Sleep>> {
        let slot_auction_ends = NetworkClock::time_until_slot(self, target_slot);
        let waiting_time = slot_auction_ends.checked_sub(deadline).unwrap_or_default();
        Box::pin(tokio::time::sleep(waiting_time))
    }

    /// From reth::chain returns a NetworkClock, returns an error if the chain is not supported
    pub fn try_from_chain(chain: Chain) -> Result<NetworkClock, NetworkClockErr> {
        let (genesis_ts, seconds_per_slot) =
            if chain.eq(&Chain::from_named(NamedChain::Mainnet)) | chain.eq(&Chain::from_id(1)) {
                (MAINNET_GENESIS_TS, SECONDS_PER_SLOT)
            } else if chain.eq(&Chain::from_named(NamedChain::Holesky)) {
                (HOLESKY_GENESIS_TS, SECONDS_PER_SLOT)
            } else if chain == Chain::from_id(32382) {
                let path = env::current_dir().unwrap();
                (devnet_genesis_ts(path)?, SECONDS_PER_SLOT)
            } else {
                return Err(NetworkClockErr::UnsupportedChain(chain.to_string()));
            };
        Ok(NetworkClock::new(genesis_ts, seconds_per_slot))
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct GenesisFileData {
    #[serde(with = "u64_hex_or_decimal")]
    timestamp: u64,
}

fn devnet_genesis_ts(mut path: PathBuf) -> Result<u64, serde_json::Error> {
    path.push("devnet/execution/genesis.json");
    let file = File::open(path).expect("Not found file");
    let reader = BufReader::new(file);
    let genesis: GenesisFileData = serde_json::from_reader(reader)?;
    Ok(genesis.timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::Path;
    use std::time::Instant;

    async fn can_sleep_until_next_slot(clock: NetworkClock) {
        let slot = NetworkClock::current_slot(&clock).unwrap() + 1;
        let deadline = Duration::from_secs(1);

        let expected_time = NetworkClock::time_until_slot(&clock, slot).checked_sub(deadline);
        let now = Instant::now();
        clock.slot_deadline(slot, deadline).await;
        assert!(
            now.elapsed().as_secs_f64().round() == expected_time.unwrap().as_secs_f64().round()
        );
    }

    #[tokio::test]
    async fn slot_by_timestamp() {
        let mainnet_clock = NetworkClock::try_from_chain(Chain::mainnet()).unwrap();
        let slot = mainnet_clock.current_slot().unwrap();
        mainnet_clock.slot_timestamp(slot);
    }

    #[tokio::test]
    async fn can_sleep_mainnet() {
        let mainnet_clock = NetworkClock::try_from_chain(Chain::mainnet()).unwrap();
        can_sleep_until_next_slot(mainnet_clock).await;
    }

    #[tokio::test]
    async fn can_read_devnet_genesis_file() {
        let mut path = env::current_dir().unwrap();
        path.pop();
        path.pop();
        assert!(devnet_genesis_ts(path).is_ok());
    }

    #[tokio::test]
    async fn unsupported_chain() {
        let unsupported_chain = Chain::from_id(12);
        assert!(NetworkClock::try_from_chain(unsupported_chain).is_err());
        assert_eq!(
            NetworkClock::try_from_chain(unsupported_chain)
                .unwrap_err()
                .to_string(),
            "Unsupported chain: 12"
        )
    }

    // Function to set up the test environment
    fn setup_test_environment() {
        let directory_path = Path::new("devnet/execution");
        fs::create_dir_all(directory_path).expect("Failed to create test directory");
        let genesis_data = r#"{ "timestamp": 1234567890 }"#;
        let file_path = directory_path.join("genesis.json");
        let mut file = File::create(file_path).expect("Failed to create test genesis.json file");
        write!(file, "{}", genesis_data).expect("Failed to write to test genesis.json file");
    }

    // Function to tear down the test environment
    fn teardown_test_environment() {
        fs::remove_file("devnet/execution/genesis.json")
            .expect("Failed to remove test genesis.json file");
        fs::remove_dir_all("devnet").expect("Failed to remove test directory");
    }

    #[test]
    fn test_devnet_genesis_ts() {
        setup_test_environment();
        NetworkClock::try_from_chain(Chain::from_id(32382)).unwrap();
        teardown_test_environment();
    }
}
