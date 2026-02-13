use lazy_static::lazy_static;
use std::{env, fmt};
use url::Url;

#[derive(Debug, Clone, PartialEq)]
pub enum RelayName {
    Flashbots,
    UltraSoundMoney,
    BloxrouteMaxprofit,
    Agnostic,
    Aestus,
    EdenNetwork,
    MerkleInternal,
    Devnet,
    MerkleRelay,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SubmissionKind {
    Ssz,
    Json,
    Sszgzip,
    /// GRPC submission with its URL
    Grpc(String),
}

impl fmt::Display for RelayName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelayName::Flashbots => f.write_str("flashbots"),
            RelayName::UltraSoundMoney => f.write_str("ultrasound"),
            RelayName::BloxrouteMaxprofit => f.write_str("bloxroute(maxprofit)"),
            RelayName::Agnostic => f.write_str("agnostic"),
            RelayName::Aestus => f.write_str("aestus"),
            RelayName::EdenNetwork => f.write_str("eden"),
            RelayName::MerkleInternal => f.write_str("merkle-internal"),
            RelayName::Devnet => f.write_str("devnet"),
            RelayName::MerkleRelay => f.write_str("merkle-relay"),
            RelayName::Unknown => f.write_str("unknown"),
        }
    }
}

lazy_static! {
    static ref FLASHBOTS_URL: Url = Url::parse("https://boost-relay.flashbots.net").unwrap();
    static ref ULTRASOUNDMONEY_URL: Url = Url::parse("https://relay.ultrasound.money").unwrap();
    static ref BLOXROUTE_REGULATED: Url =
        Url::parse("https://bloxroute.regulated.blxrbdn.com").unwrap();
    static ref BLOXROUTE_MAXPROFIT_URL: Url =
        Url::parse("https://bloxroute.max-profit.blxrbdn.com").unwrap();
    static ref AGNOSTIC_URL: Url = Url::parse("https://agnostic-relay.net").unwrap();
    static ref AESTUS_URL: Url = Url::parse("https://aestus.live").unwrap();
    static ref EDEN_NETWORK_URL: Url = Url::parse("https://relay.edennetwork.io").unwrap();
    static ref MERKLE_INTERNAL_URL: Url = Url::parse(
        &env::var("INTERNAL_RELAY_SERVER_URL").unwrap_or("http://0.0.0.0:8008".to_string())
    )
    .unwrap();
    static ref MERKLE_RELAY_URL: Url = Url::parse(
        &env::var("MERKLE_RELAY_URL").unwrap_or("http://0.0.0.0:9063".to_string())
    )
    .unwrap();
    static ref MERKLE_RELAY_GRPC_URL: Url = Url::parse(
        &env::var("MERKLE_RELAY_GRPC_URL").unwrap_or("http://0.0.0.0:50051".to_string())
    )
    .unwrap();
    static ref DEVNET_URL: Url = Url::parse(
        &env::var("DEVNET_RELAY_URL").unwrap_or("http://0.0.0.0:9062".to_string())
    )
    .unwrap();
    pub static ref DEVNET: RelayInfo = RelayInfo {
        name: RelayName::Devnet,
        url: DEVNET_URL.clone(),
        submission: SubmissionKind::Sszgzip,

    };
    pub static ref MERKLE_RELAY: RelayInfo = RelayInfo {
        name: RelayName::MerkleRelay,
        url: MERKLE_RELAY_URL.clone(),
        submission: SubmissionKind::Grpc(MERKLE_RELAY_GRPC_URL.to_string()),

    };
    pub static ref INTERNAL: RelayInfo = RelayInfo {
        name: RelayName::MerkleInternal,
        url: MERKLE_INTERNAL_URL.clone(),
        submission: SubmissionKind::Json,
    };
    pub static ref LIVE_RELAYS: Vec<RelayInfo> = vec![
        RelayInfo {
            name: RelayName::Flashbots,
            url: FLASHBOTS_URL.clone(),
            submission: SubmissionKind::Sszgzip,
        },
        RelayInfo {
            name: RelayName::UltraSoundMoney,
            url: ULTRASOUNDMONEY_URL.clone(),
            submission: SubmissionKind::Sszgzip,
        },
        // RelayInfo {
        //     name: RelayName::BloxrouteMaxprofit,
        //     url: BLOXROUTE_MAXPROFIT_URL.clone(),
        //     submission: SubmissionKind::Sszgzip,
        // },
        RelayInfo {
            name: RelayName::Agnostic,
            url: AGNOSTIC_URL.clone(),
            submission: SubmissionKind::Sszgzip,
        },
        RelayInfo {
            name: RelayName::Aestus,
            url: AESTUS_URL.clone(),
            submission: SubmissionKind::Sszgzip,

        },
        // RelayInfo {
        //     name: RelayName::EdenNetwork,
        //     url: EDEN_NETWORK_URL.clone(),
        //     submission: SubmissionKind::Sszgzip,
        // },
    ];
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelayInfo {
    name: RelayName,
    url: Url,
    submission: SubmissionKind,
}

impl RelayInfo {
    pub fn new(name: RelayName, url: Url, submission: SubmissionKind) -> Self {
        Self {
            name,
            url,
            submission,
        }
    }
    pub fn name(&self) -> &RelayName {
        &self.name
    }

    pub fn url(&self) -> &Url {
        &self.url
    }
    pub fn submission(&self) -> &SubmissionKind {
        &self.submission
    }
}
#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_display {
        ($variant:ident, $expected:expr) => {
            #[test]
            fn $variant() {
                let name = RelayName::$variant;
                assert_eq!(name.to_string(), $expected);
            }
        };
    }

    test_display!(Flashbots, "flashbots");
    test_display!(UltraSoundMoney, "ultrasound");
    test_display!(BloxrouteMaxprofit, "bloxroute(maxprofit)");
    test_display!(Agnostic, "agnostic");
    test_display!(Aestus, "aestus");
    test_display!(EdenNetwork, "eden");
    test_display!(MerkleInternal, "merkle-internal");
    test_display!(Unknown, "unknown");
}
