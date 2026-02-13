use reth::{primitives::BloomInput, revm::primitives::FixedBytes};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use tree_hash::Hash256;

pub mod bid;
pub mod blst;
pub mod build;
pub mod payload;
pub mod relays;
pub mod rpc;
pub mod run_mode;
pub mod signer;
pub mod ssz;
pub mod validator;

#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct Withdrawl(Bytes);

#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct Bytes(reth::primitives::Bytes);

impl AsRef<reth::primitives::Bytes> for Bytes {
    fn as_ref(&self) -> &reth::primitives::Bytes {
        &self.0
    }
}

impl From<reth::primitives::Bytes> for Bytes {
    fn from(bytes: reth::primitives::Bytes) -> Self {
        Self(bytes)
    }
}

#[derive(Debug, Default, PartialEq, Copy, Clone, Serialize, Deserialize)]
pub struct Bloom(reth::primitives::Bloom);

impl AsRef<reth::primitives::Bloom> for Bloom {
    fn as_ref(&self) -> &reth::primitives::Bloom {
        &self.0
    }
}

impl From<reth::primitives::Bloom> for Bloom {
    fn from(bloom: reth::primitives::Bloom) -> Self {
        Self(bloom)
    }
}

impl From<Hash256> for Bloom {
    fn from(hash: Hash256) -> Self {
        let hash = hash.as_fixed_bytes();
        let fixed_bytes_32 = FixedBytes::<32>::from(hash);

        let input = BloomInput::Hash(fixed_bytes_32);

        let bloom = reth::primitives::Bloom::from(input);

        Bloom(bloom)
    }
}

#[derive(Debug, Default, PartialEq, Copy, Clone, Serialize, Deserialize, Eq)]
pub struct B256(FixedBytes<32>);

impl AsRef<FixedBytes<32>> for B256 {
    fn as_ref(&self) -> &FixedBytes<32> {
        &self.0
    }
}

impl From<B256> for Hash256 {
    fn from(alloy: B256) -> Self {
        let bytes: [u8; 32] = alloy.0.into();
        Self::from(bytes)
    }
}

impl Display for B256 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Hash256> for B256 {
    fn from(hash: Hash256) -> Self {
        let hash = hash.as_fixed_bytes();
        Self(FixedBytes::<32>::from(hash))
    }
}
impl From<FixedBytes<32>> for B256 {
    fn from(hash: FixedBytes<32>) -> Self {
        Self(hash)
    }
}

#[derive(Debug, PartialEq, Clone, Default, Copy, Serialize, Deserialize)]
pub struct U64(reth::primitives::U64);

impl AsRef<reth::primitives::U64> for U64 {
    fn as_ref(&self) -> &reth::primitives::U64 {
        &self.0
    }
}

impl From<reth::primitives::U64> for U64 {
    fn from(uinteger: reth::primitives::U64) -> Self {
        Self(uinteger)
    }
}

#[derive(Debug, PartialEq, Clone, Default, Copy, Serialize, Deserialize, Eq, PartialOrd, Ord)]
pub struct U256(reth::primitives::U256);

impl AsRef<reth::primitives::U256> for U256 {
    fn as_ref(&self) -> &reth::primitives::U256 {
        &self.0
    }
}

impl From<reth::primitives::U256> for U256 {
    fn from(u256: reth::primitives::U256) -> Self {
        Self(u256)
    }
}

#[derive(Debug, PartialEq, Default, Clone, Copy, Serialize, Deserialize)]
pub struct Address(reth::primitives::Address);

impl AsRef<reth::primitives::Address> for Address {
    fn as_ref(&self) -> &reth::primitives::Address {
        &self.0
    }
}

impl From<reth::primitives::Address> for Address {
    fn from(address: reth::primitives::Address) -> Self {
        Self(address)
    }
}
