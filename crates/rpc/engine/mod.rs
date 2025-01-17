use ethereum_rust_core::{
    types::{ExecutionPayloadV3, PayloadStatus, PayloadValidationStatus},
    H256,
};
use serde_json::{json, Value};
use tracing::info;

use crate::RpcErr;

pub type ExchangeCapabilitiesRequest = Vec<String>;

pub struct NewPayloadV3Request {
    pub payload: ExecutionPayloadV3,
    pub expected_blob_versioned_hashes: Vec<H256>,
    pub parent_beacon_block_root: H256,
}

pub fn exchange_capabilities(capabilities: &ExchangeCapabilitiesRequest) -> Result<Value, RpcErr> {
    Ok(json!(capabilities))
}

pub fn forkchoice_updated_v3() -> Result<Value, RpcErr> {
    Ok(json!({
        "payloadId": null,
        "payloadStatus": {
            "latestValidHash": null,
            "status": "SYNCING",
            "validationError": null
        }
    }))
}

pub fn new_payload_v3(request: NewPayloadV3Request) -> Result<PayloadStatus, RpcErr> {
    let block_hash = request.payload.block_hash;

    info!("Received new payload with block hash: {}", block_hash);

    let (block_header, block_body) =
        match request.payload.into_block(request.parent_beacon_block_root) {
            Ok(block) => block,
            Err(error) => {
                return Ok(PayloadStatus {
                    status: PayloadValidationStatus::Invalid,
                    latest_valid_hash: Some(H256::zero()),
                    validation_error: Some(error.to_string()),
                })
            }
        };

    // Payload Validation

    // Check timestamp does not fall within the time frame of the Cancun fork
    let cancun_time = 0; // Placeholder -> we should fetch this from genesis?
    if block_header.timestamp <= cancun_time {
        return Err(RpcErr::UnsuportedFork);
    }
    // Check that block_hash is valid
    let actual_block_hash = block_header.compute_block_hash();
    if block_hash != actual_block_hash {
        return Ok(PayloadStatus {
            status: PayloadValidationStatus::Invalid,
            latest_valid_hash: None,
            validation_error: Some("Invalid block hash".to_string()),
        });
    }
    info!("Block hash {} is valid", block_hash);
    // Concatenate blob versioned hashes lists (tx.blob_versioned_hashes) of each blob transaction included in the payload, respecting the order of inclusion
    // and check that the resulting array matches expected_blob_versioned_hashes
    let blob_versioned_hashes: Vec<H256> = block_body
        .transactions
        .iter()
        .flat_map(|tx| tx.blob_versioned_hashes())
        .collect();
    if request.expected_blob_versioned_hashes != blob_versioned_hashes {
        return Ok(PayloadStatus {
            status: PayloadValidationStatus::Invalid,
            latest_valid_hash: None,
            validation_error: Some("Invalid blob_versioned_hashes".to_string()),
        });
    }

    Ok(PayloadStatus {
        status: PayloadValidationStatus::Valid,
        latest_valid_hash: Some(block_hash),
        validation_error: None,
    })
}
