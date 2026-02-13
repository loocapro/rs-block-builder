/// https://github.com/sigp/lighthouse/blob/dcd69dfc628cad5998225d5100b222458f3f0ecb/crypto/bls
/// We can try to remove these files once we are on latest reth and see if lighthouse is compatible
use crate::{Address, B256, U256};

use blst::BLST_ERROR as BlstError;
use serde_derive::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash::{Hash256, PackedEncoding, TreeHash, TreeHashType};
use tree_hash_derive::TreeHash;

#[macro_use]
pub mod macros;
pub mod public_key;
pub mod secret_key;
pub mod signature;

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// BLST library errors
    Blst(BlstError),
    /// The provided bytes were an incorrect length.
    InvalidByteLength { got: usize, expected: usize },
    /// The provided secret key bytes were an incorrect length.
    InvalidSecretKeyLength { got: usize, expected: usize },
    /// The public key represents the point at infinity, which is invalid.
    InvalidInfinityPublicKey,
    /// The secret key is all zero bytes, which is invalid.
    InvalidZeroSecretKey,
}

impl From<BlstError> for Error {
    fn from(e: BlstError) -> Error {
        Error::Blst(e)
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SigningData {
    pub object_root: Hash256,
    pub domain: Hash256,
}

pub trait SignedRoot: TreeHash {
    fn signing_root(&self, domain: Hash256) -> Hash256 {
        SigningData {
            object_root: self.tree_hash_root(),
            domain,
        }
        .tree_hash_root()
    }
}

impl TreeHash for B256 {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        PackedEncoding::from_slice(self.0.as_slice())
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> Hash256 {
        let me = *self;
        me.into()
    }
}

impl TreeHash for U256 {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Basic
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        let result = self.0.as_le_slice();
        PackedEncoding::from_slice(result)
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> Hash256 {
        let result = self.0.as_le_slice();
        Hash256::from_slice(result)
    }
}

impl TreeHash for Address {
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        let mut result = [0; 32];
        result[0..20].copy_from_slice(self.0.as_slice());
        PackedEncoding::from_slice(&result)
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> Hash256 {
        let mut result = [0; 32];
        result[0..20].copy_from_slice(self.0.as_slice());
        Hash256::from_slice(&result)
    }
}
