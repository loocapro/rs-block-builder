use crate::{Address, Bloom, Bytes, B256, U256, U64};
use reth::revm::primitives::FixedBytes;
use ssz::Encode;
use ssz::{Decode, DecodeError};

use tree_hash::Hash256;

impl Encode for Bloom {
    fn is_ssz_fixed_len() -> bool {
        true
    }
    fn ssz_fixed_len() -> usize {
        256
    }
    fn ssz_bytes_len(&self) -> usize {
        256
    }
    fn ssz_append(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.0 .0 .0);
    }
}

impl Decode for Bloom {
    fn is_ssz_fixed_len() -> bool {
        true
    }
    fn ssz_fixed_len() -> usize {
        256
    }
    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() != 256 {
            return Err(DecodeError::InvalidByteLength {
                len: bytes.len(),
                expected: 256,
            });
        }
        let mut bloom_data = FixedBytes::<256>::default();
        bloom_data.copy_from_slice(bytes);
        Ok(Bloom(reth::primitives::Bloom(bloom_data)))
    }
}

impl Encode for B256 {
    fn is_ssz_fixed_len() -> bool {
        <Hash256 as Encode>::is_ssz_fixed_len()
    }
    fn ssz_fixed_len() -> usize {
        <Hash256 as Encode>::ssz_fixed_len()
    }
    fn ssz_bytes_len(&self) -> usize {
        self.0.ssz_bytes_len()
    }
    fn ssz_append(&self, buf: &mut Vec<u8>) {
        self.0.ssz_append(buf)
    }
}

impl Decode for B256 {
    fn is_ssz_fixed_len() -> bool {
        <Hash256 as Decode>::is_ssz_fixed_len()
    }
    fn ssz_fixed_len() -> usize {
        <Hash256 as Decode>::ssz_fixed_len()
    }
    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        Hash256::from_ssz_bytes(bytes).map(B256::from)
    }
}

impl Encode for U256 {
    fn is_ssz_fixed_len() -> bool {
        true
    }
    fn ssz_fixed_len() -> usize {
        32
    }
    fn ssz_bytes_len(&self) -> usize {
        32
    }
    fn ssz_append(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(self.0.as_le_slice());
    }
}

impl Encode for Bytes {
    fn is_ssz_fixed_len() -> bool {
        false
    }

    fn ssz_bytes_len(&self) -> usize {
        self.0.len()
    }

    fn ssz_append(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.0);
    }
    fn as_ssz_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl Decode for Bytes {
    fn is_ssz_fixed_len() -> bool {
        false
    }

    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        Ok(Bytes(bytes.to_vec().into()))
    }
}

impl Encode for U64 {
    fn is_ssz_fixed_len() -> bool {
        true
    }
    fn ssz_fixed_len() -> usize {
        8
    }
    fn ssz_bytes_len(&self) -> usize {
        8
    }
    fn ssz_append(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(self.0.as_le_slice());
    }
}

impl Decode for U64 {
    fn is_ssz_fixed_len() -> bool {
        true
    }
    fn ssz_fixed_len() -> usize {
        8
    }
    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        let len = bytes.len();
        let expected = <Self as Decode>::ssz_fixed_len();

        if len != expected {
            Err(DecodeError::InvalidByteLength { len, expected })
        } else {
            Ok(reth::primitives::U64::try_from_le_slice(bytes)
                .expect("U256 from ssz bytes")
                .into())
        }
    }
}

impl Decode for U256 {
    fn is_ssz_fixed_len() -> bool {
        true
    }
    fn ssz_fixed_len() -> usize {
        32
    }
    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        let len = bytes.len();
        let expected = <Self as Decode>::ssz_fixed_len();

        if len != expected {
            Err(DecodeError::InvalidByteLength { len, expected })
        } else {
            Ok(reth::primitives::U256::try_from_le_slice(bytes)
                .expect("U256 from ssz bytes")
                .into())
        }
    }
}

impl Encode for Address {
    fn is_ssz_fixed_len() -> bool {
        true
    }

    fn ssz_fixed_len() -> usize {
        20
    }

    fn ssz_bytes_len(&self) -> usize {
        20
    }

    fn ssz_append(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(self.0.as_slice());
    }
}

impl Decode for Address {
    fn is_ssz_fixed_len() -> bool {
        true
    }

    fn ssz_fixed_len() -> usize {
        20
    }

    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        let len = bytes.len();
        let expected = <Self as Decode>::ssz_fixed_len();

        if len != expected {
            Err(DecodeError::InvalidByteLength { len, expected })
        } else {
            Ok(reth::primitives::Address::from_slice(bytes).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use reth::primitives::hex;
    use serde::{Deserialize, Serialize};
    use ssz_derive::{Decode, Encode};

    use crate::bid::AlloyWithdrawal;

    use super::*;

    impl FromStr for Bytes {
        type Err = hex::FromHexError;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            let bytes = hex::decode(s)?;
            Ok(Bytes(reth::primitives::Bytes(bytes.into())))
        }
    }

    #[test]
    fn test_ssz_bytes() {
        let bytes = Bytes::from_str("01020304").unwrap();
        let encoded = bytes.as_ssz_bytes();

        let decoded = Bytes::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(decoded, bytes);
    }
    #[test]
    fn test_ssz_bytes_vec() {
        let mut vec = vec![];
        let bytes = Bytes::from_str("01020304").unwrap();
        for _ in 0..100 {
            vec.push(bytes.clone());
        }
        let encoded = vec.as_ssz_bytes();
        let decoded: Vec<Bytes> = Vec::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(decoded, vec);
    }

    #[test]
    fn test_ssz_u256() {
        let u256 = U256::from(reth::primitives::U256::from(50));
        let encoded = u256.as_ssz_bytes();

        let mut expected = vec![0u8; 32];
        expected[0] = 50;

        assert_eq!(encoded, expected);
        let decoded = U256::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(decoded, u256);
    }

    #[test]
    fn test_ssz_u64() {
        let u_64 = U64::from(reth::primitives::U64::from(50));
        let encoded = u_64.as_ssz_bytes();

        let mut expected = vec![0u8; 8];
        expected[0] = 50;

        assert_eq!(encoded, expected);
        let decoded = U64::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(decoded, u_64);
    }

    #[test]
    fn test_ssz_address() {
        let actual = reth::primitives::Address::random();
        let address = Address(actual);
        let encoded = address.as_ssz_bytes();
        assert_eq!(encoded, address.0.to_vec());
        let decoded = Address::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(decoded, address);
    }

    #[test]
    fn test_ssz_b256() {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
        let b256 = B256(FixedBytes::from(bytes));
        let encoded = b256.as_ssz_bytes();
        assert_eq!(encoded, b256.0.to_vec());
        let decoded = B256::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(decoded, b256);
    }

    #[test]
    fn test_ssz_bloom() {
        let actual = reth::primitives::Bloom::from_str("0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();
        let bloom = Bloom(actual);
        let encoded = bloom.as_ssz_bytes();
        assert_eq!(encoded, bloom.0.to_vec());
        let decoded = Bloom::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(decoded, bloom);
    }

    #[derive(Clone, Debug, Serialize, Deserialize, Default, Encode, Decode, PartialEq)]
    pub struct TestStruct {
        pub gas_limit: U64,
        pub extra_data: Bytes,
    }
    #[test]
    fn test_ssz_struct() {
        let to_test = TestStruct::default();
        let encoded = to_test.as_ssz_bytes();
        let decoded = TestStruct::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(decoded, to_test);
    }

    #[test]
    fn test_ssz_vec_withdrawl() {
        let mut vec = vec![];
        let withdrawl = AlloyWithdrawal::default();
        for _ in 0..100 {
            vec.push(withdrawl.clone());
        }
        let encoded = vec.as_ssz_bytes();
        let decoded: Vec<AlloyWithdrawal> = Vec::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(decoded, vec);
    }
}
