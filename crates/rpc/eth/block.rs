use std::fmt::Display;

use ethereum_rust_evm::{evm_state, ExecutionResult, SpecId};
use ethereum_rust_storage::Store;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

use crate::utils::RpcErr;
use ethereum_rust_core::{
    types::{
        AccessListEntry, BlockHash, BlockNumber, BlockSerializable, GenericTransaction,
        ReceiptWithTxAndBlockInfo,
    },
    H256,
};

pub struct GetBlockByNumberRequest {
    pub block: BlockIdentifier,
    pub hydrated: bool,
}

pub struct GetBlockByHashRequest {
    pub block: BlockHash,
    pub hydrated: bool,
}

pub struct GetBlockTransactionCountByNumberRequest {
    pub block: BlockIdentifier,
}

pub struct GetTransactionByBlockNumberAndIndexRequest {
    pub block: BlockIdentifier,
    pub transaction_index: usize,
}

pub struct GetTransactionByBlockHashAndIndexRequest {
    pub block: BlockHash,
    pub transaction_index: usize,
}

pub struct GetBlockReceiptsRequest {
    pub block: BlockIdentifier,
}

pub struct GetTransactionByHashRequest {
    pub transaction_hash: H256,
}

pub struct GetTransactionReceiptRequest {
    pub transaction_hash: H256,
}

pub struct CreateAccessListRequest {
    pub transaction: GenericTransaction,
    pub block: Option<BlockIdentifier>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessListResult {
    access_list: Vec<AccessListEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(with = "ethereum_rust_core::serde_utils::u64::hex_str")]
    gas_used: u64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum BlockIdentifier {
    #[serde(with = "ethereum_rust_core::serde_utils::u64::hex_str")]
    Number(BlockNumber),
    Tag(BlockTag),
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub enum BlockTag {
    Earliest,
    Finalized,
    Safe,
    #[default]
    Latest,
    Pending,
}

impl GetBlockByNumberRequest {
    pub fn parse(params: &Option<Vec<Value>>) -> Option<GetBlockByNumberRequest> {
        let params = params.as_ref()?;
        if params.len() != 2 {
            return None;
        };
        Some(GetBlockByNumberRequest {
            block: serde_json::from_value(params[0].clone()).ok()?,
            hydrated: serde_json::from_value(params[1].clone()).ok()?,
        })
    }
}

impl GetBlockByHashRequest {
    pub fn parse(params: &Option<Vec<Value>>) -> Option<GetBlockByHashRequest> {
        let params = params.as_ref()?;
        if params.len() != 2 {
            return None;
        };
        Some(GetBlockByHashRequest {
            block: serde_json::from_value(params[0].clone()).ok()?,
            hydrated: serde_json::from_value(params[1].clone()).ok()?,
        })
    }
}

impl GetBlockTransactionCountByNumberRequest {
    pub fn parse(params: &Option<Vec<Value>>) -> Option<GetBlockTransactionCountByNumberRequest> {
        let params = params.as_ref()?;
        if params.len() != 1 {
            return None;
        };
        Some(GetBlockTransactionCountByNumberRequest {
            block: serde_json::from_value(params[0].clone()).ok()?,
        })
    }
}

impl GetTransactionByBlockNumberAndIndexRequest {
    pub fn parse(
        params: &Option<Vec<Value>>,
    ) -> Option<GetTransactionByBlockNumberAndIndexRequest> {
        let params = params.as_ref()?;
        if params.len() != 2 {
            return None;
        };
        Some(GetTransactionByBlockNumberAndIndexRequest {
            block: serde_json::from_value(params[0].clone()).ok()?,
            transaction_index: serde_json::from_value(params[1].clone()).ok()?,
        })
    }
}

impl GetTransactionByBlockHashAndIndexRequest {
    pub fn parse(params: &Option<Vec<Value>>) -> Option<GetTransactionByBlockHashAndIndexRequest> {
        let params = params.as_ref()?;
        if params.len() != 2 {
            return None;
        };
        Some(GetTransactionByBlockHashAndIndexRequest {
            block: serde_json::from_value(params[0].clone()).ok()?,
            transaction_index: serde_json::from_value(params[1].clone()).ok()?,
        })
    }
}

impl GetBlockReceiptsRequest {
    pub fn parse(params: &Option<Vec<Value>>) -> Option<GetBlockReceiptsRequest> {
        let params = params.as_ref()?;
        if params.len() != 1 {
            return None;
        };
        Some(GetBlockReceiptsRequest {
            block: serde_json::from_value(params[0].clone()).ok()?,
        })
    }
}

impl GetTransactionByHashRequest {
    pub fn parse(params: &Option<Vec<Value>>) -> Option<GetTransactionByHashRequest> {
        let params = params.as_ref()?;
        if params.len() != 1 {
            return None;
        };
        Some(GetTransactionByHashRequest {
            transaction_hash: serde_json::from_value(params[0].clone()).ok()?,
        })
    }
}

impl GetTransactionReceiptRequest {
    pub fn parse(params: &Option<Vec<Value>>) -> Option<GetTransactionReceiptRequest> {
        let params = params.as_ref()?;
        if params.len() != 1 {
            return None;
        };
        Some(GetTransactionReceiptRequest {
            transaction_hash: serde_json::from_value(params[0].clone()).ok()?,
        })
    }
}

impl CreateAccessListRequest {
    pub fn parse(params: &Option<Vec<Value>>) -> Option<CreateAccessListRequest> {
        let params = params.as_ref()?;
        if params.len() > 2 {
            return None;
        };
        let block = match params.get(1) {
            // Differentiate between missing and bad block param
            Some(value) => Some(serde_json::from_value(value.clone()).ok()?),
            None => None,
        };
        Some(CreateAccessListRequest {
            transaction: serde_json::from_value(params.first()?.clone()).ok()?,
            block,
        })
    }
}

pub fn get_block_by_number(
    request: &GetBlockByNumberRequest,
    storage: Store,
) -> Result<Value, RpcErr> {
    info!("Requested block with number: {}", request.block);
    let block_number = match request.block {
        BlockIdentifier::Tag(_) => unimplemented!("Obtain block number from tag"),
        BlockIdentifier::Number(block_number) => block_number,
    };
    let header = storage.get_block_header(block_number);
    let body = storage.get_block_body(block_number);
    let (header, body) = match (header, body) {
        (Ok(Some(header)), Ok(Some(body))) => (header, body),
        // Block not found
        (Ok(_), Ok(_)) => return Ok(Value::Null),
        // DB error
        _ => return Err(RpcErr::Internal),
    };
    let block = BlockSerializable::from_block(header, body, request.hydrated);

    serde_json::to_value(&block).map_err(|_| RpcErr::Internal)
}

pub fn get_block_by_hash(request: &GetBlockByHashRequest, storage: Store) -> Result<Value, RpcErr> {
    info!("Requested block with hash: {}", request.block);
    let block_number = match storage.get_block_number(request.block) {
        Ok(Some(number)) => number,
        Ok(_) => return Ok(Value::Null),
        _ => return Err(RpcErr::Internal),
    };
    let header = storage.get_block_header(block_number);
    let body = storage.get_block_body(block_number);
    let (header, body) = match (header, body) {
        (Ok(Some(header)), Ok(Some(body))) => (header, body),
        // Block not found
        (Ok(_), Ok(_)) => return Ok(Value::Null),
        // DB error
        _ => return Err(RpcErr::Internal),
    };
    let block = BlockSerializable::from_block(header, body, request.hydrated);

    serde_json::to_value(&block).map_err(|_| RpcErr::Internal)
}

pub fn get_block_transaction_count_by_number(
    request: &GetBlockTransactionCountByNumberRequest,
    storage: Store,
) -> Result<Value, RpcErr> {
    info!(
        "Requested transaction count for block with number: {}",
        request.block
    );
    let block_number = match request.block {
        BlockIdentifier::Tag(_) => unimplemented!("Obtain block number from tag"),
        BlockIdentifier::Number(block_number) => block_number,
    };
    let block_body = match storage.get_block_body(block_number) {
        Ok(Some(block_body)) => block_body,
        Ok(_) => return Ok(Value::Null),
        _ => return Err(RpcErr::Internal),
    };
    let transaction_count = block_body.transactions.len();

    serde_json::to_value(format!("{:#x}", transaction_count)).map_err(|_| RpcErr::Internal)
}

pub fn get_transaction_by_block_number_and_index(
    request: &GetTransactionByBlockNumberAndIndexRequest,
    storage: Store,
) -> Result<Value, RpcErr> {
    info!(
        "Requested transaction at index: {} of block with number: {}",
        request.transaction_index, request.block,
    );
    let block_number = match request.block {
        BlockIdentifier::Tag(_) => unimplemented!("Obtain block number from tag"),
        BlockIdentifier::Number(block_number) => block_number,
    };
    let block_body = match storage.get_block_body(block_number) {
        Ok(Some(block_body)) => block_body,
        Ok(_) => return Ok(Value::Null),
        _ => return Err(RpcErr::Internal),
    };
    let tx = match block_body.transactions.get(request.transaction_index) {
        Some(tx) => tx,
        None => return Ok(Value::Null),
    };

    serde_json::to_value(tx).map_err(|_| RpcErr::Internal)
}

pub fn get_transaction_by_block_hash_and_index(
    request: &GetTransactionByBlockHashAndIndexRequest,
    storage: Store,
) -> Result<Value, RpcErr> {
    info!(
        "Requested transaction at index: {} of block with hash: {}",
        request.transaction_index, request.block,
    );
    let block_number = match storage.get_block_number(request.block) {
        Ok(Some(number)) => number,
        Ok(_) => return Ok(Value::Null),
        _ => return Err(RpcErr::Internal),
    };
    let block_body = match storage.get_block_body(block_number) {
        Ok(Some(block_body)) => block_body,
        Ok(_) => return Ok(Value::Null),
        _ => return Err(RpcErr::Internal),
    };
    let tx = match block_body.transactions.get(request.transaction_index) {
        Some(tx) => tx,
        None => return Ok(Value::Null),
    };

    serde_json::to_value(tx).map_err(|_| RpcErr::Internal)
}

pub fn get_block_receipts(
    request: &GetBlockReceiptsRequest,
    storage: Store,
) -> Result<Value, RpcErr> {
    info!(
        "Requested receipts for block with number: {}",
        request.block
    );
    let block_number = match request.block {
        BlockIdentifier::Tag(_) => unimplemented!("Obtain block number from tag"),
        BlockIdentifier::Number(block_number) => block_number,
    };
    let header = storage.get_block_header(block_number);
    let body = storage.get_block_body(block_number);
    let (header, body) = match (header, body) {
        (Ok(Some(header)), Ok(Some(body))) => (header, body),
        // Block not found
        (Ok(_), Ok(_)) => return Ok(Value::Null),
        // DB error
        _ => return Err(RpcErr::Internal),
    };
    // Fetch receipt info from block
    let block_info = header.receipt_info();
    // Fetch receipt for each tx in the block and add block and tx info
    let mut receipts = Vec::new();
    for (index, tx) in body.transactions.iter().enumerate() {
        let index = index as u64;
        let receipt = match storage.get_receipt(block_number, index) {
            Ok(Some(receipt)) => receipt,
            Ok(_) => return Ok(Value::Null),
            _ => return Err(RpcErr::Internal),
        };
        let block_info = block_info.clone();
        let tx_info = tx.receipt_info(index);
        receipts.push(ReceiptWithTxAndBlockInfo {
            receipt,
            tx_info,
            block_info,
        })
    }

    serde_json::to_value(&receipts).map_err(|_| RpcErr::Internal)
}

pub fn get_transaction_by_hash(
    request: &GetTransactionByHashRequest,
    storage: Store,
) -> Result<Value, RpcErr> {
    info!(
        "Requested transaction with hash: {}",
        request.transaction_hash,
    );
    let transaction: ethereum_rust_core::types::Transaction =
        match storage.get_transaction_by_hash(request.transaction_hash) {
            Ok(Some(transaction)) => transaction,
            Ok(_) => return Ok(Value::Null),
            _ => return Err(RpcErr::Internal),
        };

    serde_json::to_value(transaction).map_err(|_| RpcErr::Internal)
}

pub fn get_transaction_receipt(
    request: &GetTransactionReceiptRequest,
    storage: Store,
) -> Result<Value, RpcErr> {
    info!(
        "Requested receipt for transaction {}",
        request.transaction_hash,
    );
    let (block_number, index) = match storage.get_transaction_location(request.transaction_hash) {
        Ok(Some(location)) => location,
        Ok(_) => return Ok(Value::Null),
        _ => return Err(RpcErr::Internal),
    };
    let block_header = match storage.get_block_header(block_number) {
        Ok(Some(block_header)) => block_header,
        Ok(_) => return Ok(Value::Null),
        _ => return Err(RpcErr::Internal),
    };
    let block_body = match storage.get_block_body(block_number) {
        Ok(Some(block_body)) => block_body,
        Ok(_) => return Ok(Value::Null),
        _ => return Err(RpcErr::Internal),
    };
    let receipt = match storage.get_receipt(block_number, index) {
        Ok(Some(receipt)) => receipt,
        Ok(_) => return Ok(Value::Null),
        _ => return Err(RpcErr::Internal),
    };
    let tx = match index
        .try_into()
        .ok()
        .and_then(|index: usize| block_body.transactions.get(index))
    {
        Some(tx) => tx,
        _ => return Ok(Value::Null),
    };
    let block_info = block_header.receipt_info();
    let tx_info = tx.receipt_info(index);
    let receipt = ReceiptWithTxAndBlockInfo {
        receipt,
        tx_info,
        block_info,
    };
    serde_json::to_value(&receipt).map_err(|_| RpcErr::Internal)
}

pub fn create_access_list(
    request: &CreateAccessListRequest,
    storage: Store,
) -> Result<Value, RpcErr> {
    let block = request.block.clone().unwrap_or_default();
    info!("Requested access list creation for tx on block: {}", block);
    let block_number = match block {
        BlockIdentifier::Tag(_) => unimplemented!("Obtain block number from tag"),
        BlockIdentifier::Number(block_number) => block_number,
    };
    let header = match storage.get_block_header(block_number) {
        Ok(Some(header)) => header,
        // Block not found
        Ok(_) => return Ok(Value::Null),
        // DB error
        _ => return Err(RpcErr::Internal),
    };
    // Run transaction and obtain access list
    let (gas_used, access_list, error) = match ethereum_rust_evm::create_access_list(
        &request.transaction,
        &header,
        &mut evm_state(storage),
        SpecId::CANCUN,
    )
    .map_err(|_| RpcErr::Vm)?
    {
        (
            ExecutionResult::Success {
                reason: _,
                gas_used,
                gas_refunded: _,
                output: _,
            },
            access_list,
        ) => (gas_used, access_list, None),
        (
            ExecutionResult::Revert {
                gas_used,
                output: _,
            },
            access_list,
        ) => (
            gas_used,
            access_list,
            Some("Transaction Reverted".to_string()),
        ),
        (ExecutionResult::Halt { reason, gas_used }, access_list) => {
            (gas_used, access_list, Some(reason))
        }
    };
    let result = AccessListResult {
        access_list: access_list
            .into_iter()
            .map(|(address, storage_keys)| AccessListEntry {
                address,
                storage_keys,
            })
            .collect(),
        error,
        gas_used,
    };

    serde_json::to_value(result).map_err(|_| RpcErr::Internal)
}

impl Display for BlockIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockIdentifier::Number(num) => num.fmt(f),
            BlockIdentifier::Tag(tag) => match tag {
                BlockTag::Earliest => "Earliest".fmt(f),
                BlockTag::Finalized => "Finalized".fmt(f),
                BlockTag::Safe => "Safe".fmt(f),
                BlockTag::Latest => "Latest".fmt(f),
                BlockTag::Pending => "Pending".fmt(f),
            },
        }
    }
}

impl Default for BlockIdentifier {
    fn default() -> BlockIdentifier {
        BlockIdentifier::Tag(BlockTag::default())
    }
}
