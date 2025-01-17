use std::{future::IntoFuture, net::SocketAddr};

use axum::{routing::post, Json, Router};
use engine::{ExchangeCapabilitiesRequest, NewPayloadV3Request};
use eth::{
    account::{self, GetBalanceRequest, GetCodeRequest, GetStorageAtRequest},
    block::{
        self, CreateAccessListRequest, GetBlockByHashRequest, GetBlockByNumberRequest,
        GetBlockReceiptsRequest, GetBlockTransactionCountByNumberRequest,
        GetTransactionByBlockHashAndIndexRequest, GetTransactionByBlockNumberAndIndexRequest,
        GetTransactionByHashRequest, GetTransactionReceiptRequest,
    },
    client,
};
use serde_json::Value;
use tokio::net::TcpListener;
use tracing::info;
use utils::{RpcErr, RpcErrorMetadata, RpcErrorResponse, RpcRequest, RpcSuccessResponse};

mod admin;
mod engine;
mod eth;
mod utils;

use axum::extract::State;
use ethereum_rust_storage::Store;

pub async fn start_api(http_addr: SocketAddr, authrpc_addr: SocketAddr, storage: Store) {
    let http_router = Router::new()
        .route("/", post(handle_http_request))
        .with_state(storage.clone());
    let http_listener = TcpListener::bind(http_addr).await.unwrap();

    let authrpc_router = Router::new()
        .route("/", post(handle_authrpc_request))
        .with_state(storage);
    let authrpc_listener = TcpListener::bind(authrpc_addr).await.unwrap();

    let authrpc_server = axum::serve(authrpc_listener, authrpc_router)
        .with_graceful_shutdown(shutdown_signal())
        .into_future();
    let http_server = axum::serve(http_listener, http_router)
        .with_graceful_shutdown(shutdown_signal())
        .into_future();

    info!("Starting HTTP server at {http_addr}");
    info!("Starting Auth-RPC server at {}", authrpc_addr);

    let _ = tokio::try_join!(authrpc_server, http_server)
        .inspect_err(|e| info!("Error shutting down servers: {:?}", e));
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
}

pub async fn handle_authrpc_request(State(storage): State<Store>, body: String) -> Json<Value> {
    let req: RpcRequest = serde_json::from_str(&body).unwrap();
    let res = match map_requests(&req, storage.clone()) {
        res @ Ok(_) => res,
        _ => map_internal_requests(&req, storage),
    };
    rpc_response(req.id, res)
}

pub async fn handle_http_request(State(storage): State<Store>, body: String) -> Json<Value> {
    let req: RpcRequest = serde_json::from_str(&body).unwrap();
    let res = map_requests(&req, storage);
    rpc_response(req.id, res)
}

/// Handle requests that can come from either clients or other users
pub fn map_requests(req: &RpcRequest, storage: Store) -> Result<Value, RpcErr> {
    match req.method.as_str() {
        "engine_exchangeCapabilities" => {
            let capabilities: ExchangeCapabilitiesRequest = req
                .params
                .as_ref()
                .ok_or(RpcErr::BadParams)?
                .first()
                .ok_or(RpcErr::BadParams)
                .and_then(|v| serde_json::from_value(v.clone()).map_err(|_| RpcErr::BadParams))?;
            engine::exchange_capabilities(&capabilities)
        }
        "eth_chainId" => client::chain_id(storage),
        "eth_syncing" => client::syncing(),
        "eth_getBlockByNumber" => {
            let request = GetBlockByNumberRequest::parse(&req.params).ok_or(RpcErr::BadParams)?;
            block::get_block_by_number(&request, storage)
        }
        "eth_getBlockByHash" => {
            let request = GetBlockByHashRequest::parse(&req.params).ok_or(RpcErr::BadParams)?;
            block::get_block_by_hash(&request, storage)
        }
        "eth_getBalance" => {
            let request = GetBalanceRequest::parse(&req.params).ok_or(RpcErr::BadParams)?;
            account::get_balance(&request, storage)
        }
        "eth_getCode" => {
            let request = GetCodeRequest::parse(&req.params).ok_or(RpcErr::BadParams)?;
            account::get_code(&request, storage)
        }
        "eth_getStorageAt" => {
            let request = GetStorageAtRequest::parse(&req.params).ok_or(RpcErr::BadParams)?;
            account::get_storage_at(&request, storage)
        }
        "eth_getBlockTransactionCountByNumber" => {
            let request = GetBlockTransactionCountByNumberRequest::parse(&req.params)
                .ok_or(RpcErr::BadParams)?;
            block::get_block_transaction_count_by_number(&request, storage)
        }
        "eth_getTransactionByBlockNumberAndIndex" => {
            let request = GetTransactionByBlockNumberAndIndexRequest::parse(&req.params)
                .ok_or(RpcErr::BadParams)?;
            block::get_transaction_by_block_number_and_index(&request, storage)
        }
        "eth_getTransactionByBlockHashAndIndex" => {
            let request = GetTransactionByBlockHashAndIndexRequest::parse(&req.params)
                .ok_or(RpcErr::BadParams)?;
            block::get_transaction_by_block_hash_and_index(&request, storage)
        }
        "eth_getBlockReceipts" => {
            let request = GetBlockReceiptsRequest::parse(&req.params).ok_or(RpcErr::BadParams)?;
            block::get_block_receipts(&request, storage)
        }
        "eth_getTransactionByHash" => {
            let request =
                GetTransactionByHashRequest::parse(&req.params).ok_or(RpcErr::BadParams)?;
            block::get_transaction_by_hash(&request, storage)
        }
        "eth_getTransactionReceipt" => {
            let request =
                GetTransactionReceiptRequest::parse(&req.params).ok_or(RpcErr::BadParams)?;
            block::get_transaction_receipt(&request, storage)
        }
        "eth_createAccessList" => {
            let request = CreateAccessListRequest::parse(&req.params).ok_or(RpcErr::BadParams)?;
            block::create_access_list(&request, storage)
        }
        "engine_forkchoiceUpdatedV3" => engine::forkchoice_updated_v3(),
        "engine_newPayloadV3" => {
            let request =
                parse_new_payload_v3_request(req.params.as_ref().ok_or(RpcErr::BadParams)?)?;
            Ok(serde_json::to_value(engine::new_payload_v3(request)?).unwrap())
        }
        "admin_nodeInfo" => admin::node_info(),
        _ => Err(RpcErr::MethodNotFound),
    }
}

/// Handle requests from other clients
pub fn map_internal_requests(_req: &RpcRequest, _storage: Store) -> Result<Value, RpcErr> {
    Err(RpcErr::MethodNotFound)
}

fn rpc_response<E>(id: i32, res: Result<Value, E>) -> Json<Value>
where
    E: Into<RpcErrorMetadata>,
{
    match res {
        Ok(result) => Json(
            serde_json::to_value(RpcSuccessResponse {
                id,
                jsonrpc: "2.0".to_string(),
                result,
            })
            .unwrap(),
        ),
        Err(error) => Json(
            serde_json::to_value(RpcErrorResponse {
                id,
                jsonrpc: "2.0".to_string(),
                error: error.into(),
            })
            .unwrap(),
        ),
    }
}

fn parse_new_payload_v3_request(params: &[Value]) -> Result<NewPayloadV3Request, RpcErr> {
    if params.len() != 3 {
        return Err(RpcErr::BadParams);
    }
    let payload = serde_json::from_value(params[0].clone()).map_err(|_| RpcErr::BadParams)?;
    let expected_blob_versioned_hashes =
        serde_json::from_value(params[1].clone()).map_err(|_| RpcErr::BadParams)?;
    let parent_beacon_block_root =
        serde_json::from_value(params[2].clone()).map_err(|_| RpcErr::BadParams)?;
    Ok(NewPayloadV3Request {
        payload,
        expected_blob_versioned_hashes,
        parent_beacon_block_root,
    })
}

#[cfg(test)]
mod tests {
    use ethereum_rust_core::{
        types::{code_hash, AccountInfo, BlockHeader},
        Address, Bytes, U256,
    };
    use ethereum_rust_storage::EngineType;
    use std::str::FromStr;

    use super::*;

    // Maps string rpc response to RpcSuccessResponse as serde Value
    // This is used to avoid failures due to field order and allow easier string comparisons for responses
    fn to_rpc_response_success_value(str: &str) -> serde_json::Value {
        serde_json::to_value(serde_json::from_str::<RpcSuccessResponse>(str).unwrap()).unwrap()
    }

    #[test]
    fn create_access_list_simple_transfer() {
        // Create Request
        // Request taken from https://github.com/ethereum/execution-apis/blob/main/tests/eth_createAccessList/create-al-value-transfer.io
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"eth_createAccessList","params":[{"from":"0x0c2c51a0990aee1d73c1228de158688341557508","nonce":"0x0","to":"0x0100000000000000000000000000000000000000","value":"0xa"},"0x00"]}"#;
        let request: RpcRequest = serde_json::from_str(body).unwrap();
        // Setup initial storage
        let storage =
            Store::new("temp.db", EngineType::InMemory).expect("Failed to create test DB");
        // Values taken from https://github.com/ethereum/execution-apis/blob/main/tests/genesis.json
        // TODO: Replace this initialization with reading and storing genesis block
        storage
            .add_block_header(0, BlockHeader::default())
            .expect("Failed to write to test DB");
        let address = Address::from_str("0c2c51a0990aee1d73c1228de158688341557508").unwrap();
        let account_info = AccountInfo {
            balance: U256::from_str_radix("c097ce7bc90715b34b9f1000000000", 16).unwrap(),
            ..Default::default()
        };
        storage
            .add_account_info(address, account_info)
            .expect("Failed to write to test DB");
        // Process request
        let result = map_requests(&request, storage);
        let response = rpc_response(request.id, result);
        let expected_response = to_rpc_response_success_value(
            r#"{"jsonrpc":"2.0","id":1,"result":{"accessList":[],"gasUsed":"0x5208"}}"#,
        );
        assert_eq!(response.to_string(), expected_response.to_string());
    }

    #[test]
    fn create_access_list_create() {
        // Create Request
        // Request taken from https://github.com/ethereum/execution-apis/blob/main/tests/eth_createAccessList/create-al-contract.io
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"eth_createAccessList","params":[{"from":"0x0c2c51a0990aee1d73c1228de158688341557508","gas":"0xea60","gasPrice":"0x44103f2","input":"0x010203040506","nonce":"0x0","to":"0x7dcd17433742f4c0ca53122ab541d0ba67fc27df"},"0x00"]}"#;
        let request: RpcRequest = serde_json::from_str(body).unwrap();
        // Setup initial storage
        let storage =
            Store::new("temp.db", EngineType::InMemory).expect("Failed to create test DB");
        // Values taken from https://github.com/ethereum/execution-apis/blob/main/tests/genesis.json
        // TODO: Replace this initialization with reading and storing genesis block
        storage
            .add_block_header(0, BlockHeader::default())
            .expect("Failed to write to test DB");
        let address = Address::from_str("0c2c51a0990aee1d73c1228de158688341557508").unwrap();
        let account_info = AccountInfo {
            balance: U256::from_str_radix("c097ce7bc90715b34b9f1000000000", 16).unwrap(),
            ..Default::default()
        };
        storage
            .add_account_info(address, account_info)
            .expect("Failed to write to test DB");
        let address = Address::from_str("7dcd17433742f4c0ca53122ab541d0ba67fc27df").unwrap();
        let code = Bytes::copy_from_slice(
            &hex::decode("3680600080376000206000548082558060010160005560005263656d697460206000a2")
                .unwrap(),
        );
        let code_hash = code_hash(&code);
        let account_info = AccountInfo {
            code_hash,
            ..Default::default()
        };
        storage
            .add_account_info(address, account_info)
            .expect("Failed to write to test DB");
        storage
            .add_account_code(code_hash, code)
            .expect("Failed to write to test DB");
        // Process request
        let result = map_requests(&request, storage);
        let response =
            serde_json::from_value::<RpcSuccessResponse>(rpc_response(request.id, result).0)
                .expect("Request failed");
        let expected_response_string = r#"{"jsonrpc":"2.0","id":1,"result":{"accessList":[{"address":"0x7dcd17433742f4c0ca53122ab541d0ba67fc27df","storageKeys":["0x0000000000000000000000000000000000000000000000000000000000000000","0x13a08e3cd39a1bc7bf9103f63f83273cced2beada9f723945176d6b983c65bd2"]}],"gasUsed":"0xca3c"}}"#;
        let expected_response =
            serde_json::from_str::<RpcSuccessResponse>(expected_response_string).unwrap();
        // Due to the scope of this test, we don't have the full state up to date which can cause variantions in gas used due to the difference in the blockchain state
        // So we will skip checking the gas_used and only check that the access list is correct
        // The gas_used will be checked when running the hive test framework
        assert_eq!(
            response.result["accessList"],
            expected_response.result["accessList"]
        )
    }
}
