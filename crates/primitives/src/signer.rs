use crate::blst::{public_key::BlsPublicKey, secret_key::BlsSecretKey, signature::BlsSignature};
use serde_derive::{Deserialize, Serialize};
use ssz_derive::Decode;
use ssz_derive::Encode;

use tree_hash::{Hash256, TreeHash};
use tree_hash_derive::TreeHash;

use crate::blst::SignedRoot;

type Domain = [u8; 32];

#[derive(Debug, Clone)]
pub struct BuilderDomain([u8; 4]);

impl Default for BuilderDomain {
    fn default() -> Self {
        BuilderDomain([0, 0, 0, 1])
    }
}

impl From<BuilderDomain> for [u8; 4] {
    fn from(val: BuilderDomain) -> Self {
        val.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct ForkVersion([u8; 4]);

impl ForkVersion {
    pub fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }
}
impl AsRef<[u8]> for ForkVersion {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<ForkVersion> for [u8; 4] {
    fn from(val: ForkVersion) -> Self {
        val.0
    }
}
#[derive(Debug, Clone)]
pub struct BlsSigner {
    secret_key: BlsSecretKey,
    fork_version: ForkVersion,
    genesis_validators_root: Hash256,
}

impl Default for BlsSigner {
    fn default() -> Self {
        Self {
            secret_key: BlsSecretKey::random(),
            fork_version: ForkVersion::default(),
            genesis_validators_root: Hash256::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ForkData {
    #[serde(with = "serde_utils::bytes_4_hex")]
    pub current_version: [u8; 4],
    pub genesis_validators_root: Hash256,
}

impl BlsSigner {
    pub fn new(secret_key: BlsSecretKey, fork_version: ForkVersion) -> Self {
        Self {
            secret_key,
            fork_version,
            genesis_validators_root: Hash256::default(),
        }
    }

    pub fn sign<T: SignedRoot>(&self, message: &mut T) -> BlsSignature {
        let domain = &self.compute_builder_domain();
        let signing_root = message.signing_root(domain.into());
        let sk = &self.secret_key;
        BlsSecretKey::sign(sk, signing_root)
    }
    pub fn public_key(&self) -> BlsPublicKey {
        self.secret_key.public_key()
    }
    pub fn compute_builder_domain(&self) -> Domain {
        let fork_data_root = ForkData {
            current_version: self.fork_version.clone().into(),
            genesis_validators_root: self.genesis_validators_root,
        }
        .tree_hash_root();
        let mut domain = Domain::default();
        domain[..4].copy_from_slice(&BuilderDomain::default().0);
        domain[4..].copy_from_slice(&fork_data_root.as_ref()[..28]);
        domain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reth::primitives::hex;

    #[test]
    fn can_compute_b_domain() {
        let signer = BlsSigner::new(BlsSecretKey::random(), ForkVersion::default());
        let domain = signer.compute_builder_domain();
        let to_string = hex::encode(domain);
        let expected = "00000001f5a5fd42d16a20302798ef6ed309979b43003d2320d9f0e8ea9831a9";
        assert_eq!(to_string, expected);
        let devnet_fork_version =
            "00000001e4b3c03845c9cd15780441feed2a67053b9cc4ae1547af5747433120";
        let signer = BlsSigner::new(
            BlsSecretKey::random(),
            ForkVersion([0x20, 0x00, 0x00, 0x89]),
        );
        let domain = signer.compute_builder_domain();
        let to_string = hex::encode(domain);
        assert_eq!(to_string, devnet_fork_version);
    }

    #[derive(Default, Debug, Serialize, Deserialize, Encode, Decode, TreeHash)]
    struct SomethingElse {
        pub inner: u64,
    }

    impl SignedRoot for SomethingElse {}
    #[test]
    fn can_sign() {
        let sk = BlsSecretKey::random();
        let pk = &sk.public_key();

        let signer = BlsSigner::new(sk, ForkVersion::default());
        let domain = signer.compute_builder_domain();

        let mut to_sign = SomethingElse::default();
        let sig = &signer.sign(&mut to_sign);

        let is_verified = sig.verify(pk, to_sign.signing_root(domain.into()));

        assert!(is_verified);
    }
}
