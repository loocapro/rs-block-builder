mod mevrelay;
use builder_primitives::bid::SignedBid;
pub use mevrelay::*;

impl From<SignedBid> for SubmitBlockRequest {
    fn from(value: SignedBid) -> Self {
        let bid_trace = BidTrace {
            slot: value.message.slot,
            parent_hash: value.message.parent_hash.as_ref().to_vec(),
            block_hash: value.message.block_hash.as_ref().to_vec(),
            builder_pubkey: value.message.builder_pubkey.serialize().to_vec(),
            proposer_pubkey: value.message.proposer_pubkey.serialize().to_vec(),
            proposer_fee_recipient: value.message.proposer_fee_recipient.as_ref().to_vec(),
            gas_limit: value.message.gas_limit,
            gas_used: value.message.gas_used,
            value: value.message.value.as_ref().to_string(),
        };
        let exec_payload = ExecutionPayload {
            parent_hash: value.execution_payload.parent_hash.as_ref().to_vec(),
            state_root: value.execution_payload.state_root.as_ref().to_vec(),
            receipts_root: value.execution_payload.receipts_root.as_ref().to_vec(),
            logs_bloom: value.execution_payload.logs_bloom.as_ref().to_vec(),
            prev_randao: value.execution_payload.prev_randao.as_ref().to_vec(),
            extra_data: value.execution_payload.extra_data.as_ref().to_vec(),
            base_fee_per_gas: value
                .execution_payload
                .base_fee_per_gas
                .as_ref()
                .to_be_bytes_vec(),
            fee_recipient: value.execution_payload.fee_recipient.as_ref().to_vec(),
            block_hash: value.execution_payload.block_hash.as_ref().to_vec(),
            transactions: value
                .execution_payload
                .transactions
                .into_iter()
                .map(|t| CompressTx {
                    raw_data: t.as_ref().to_vec(),
                })
                .collect(),
            withdrawals: value
                .execution_payload
                .withdrawals
                .into_iter()
                .map(|w| Withdrawal {
                    index: w.index,
                    validator_index: w.validator_index,
                    address: w.address.as_ref().to_vec(),
                    amount: w.amount,
                })
                .collect(),
            block_number: value.execution_payload.block_number,
            gas_limit: value.execution_payload.gas_limit,
            timestamp: value.execution_payload.timestamp,
            gas_used: value.execution_payload.gas_used,
            blob_gas_used: value.execution_payload.blob_gas_used,
            excess_blob_gas: value.execution_payload.excess_blob_gas,
        };
        let blobs_bundle = BlobsBundle {
            commitments: value
                .blobs_bundle
                .commitments
                .into_iter()
                .map(|c| c.to_vec())
                .collect(),
            proofs: value
                .blobs_bundle
                .proofs
                .into_iter()
                .map(|c| c.to_vec())
                .collect(),
            blobs: value
                .blobs_bundle
                .blobs
                .into_iter()
                .map(|c| c.to_vec())
                .collect(),
        };
        SubmitBlockRequest {
            bid_trace: Some(bid_trace),
            execution_payload: Some(exec_payload),
            signature: value.signature.serialize().to_vec(),
            blobs_bundle: Some(blobs_bundle),
        }
    }
}
