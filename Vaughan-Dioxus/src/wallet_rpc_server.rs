use std::sync::Arc;
use std::collections::HashSet;
use std::str::FromStr;
use axum::{
    routing::{get, post},
    Router, Json, middleware,
    response::IntoResponse,
    http::{StatusCode, Method},
    extract::State,
};
use tower_http::cors::CorsLayer;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tokio::net::TcpListener;
use alloy::signers::SignerSync;
use alloy::primitives::U256;

use vaughan_ipc_types::{
    AccountInfo, CustomDappInfo, NetworkInfo, SignMessagePayload, SignTxPayload, AddTrustedHostPayload,
};
use crate::services::AppServices;
use crate::dapp_approval::{broker, ApprovalDecision};
use vaughan_core::chains::EvmTransaction;
use vaughan_core::core::transaction::TransactionService;
use vaughan_core::core::{address_to_hex, load_signer_for_address, parse_optional_u64_decimal, AccountType};
use vaughan_core::core::ambire_abi::AmbireAccount;
use vaughan_core::core::smart_account::{build_init_code, AMBIRE_ACCOUNT_BYTECODE};
use vaughan_core::core::scw_transaction::{
    build_signed_execute, build_signed_deploy_and_execute,
    get_smart_account_nonce, is_account_deployed,
};
use vaughan_core::chains::evm::utils::parse_address;

#[derive(Clone)]
pub struct WalletRpcState {
    pub services: AppServices,
    pub token: String,
}

pub struct WalletRpcServer {
    tx_shutdown: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl WalletRpcServer {
    pub fn start(services: AppServices) -> Result<(Self, u16, String), String> {
        let (tx_port_token, rx_port_token) = std::sync::mpsc::channel();
        let (tx_shutdown, rx_shutdown) = tokio::sync::oneshot::channel::<()>();
        let token = Uuid::new_v4().to_string();
        let token_clone = token.clone();

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build() {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx_port_token.send(Err(e.to_string()));
                    return;
                }
            };

            rt.block_on(async {
                let listener = match TcpListener::bind("127.0.0.1:0").await {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = tx_port_token.send(Err(e.to_string()));
                        return;
                    }
                };

                let local_addr = match listener.local_addr() {
                    Ok(addr) => addr,
                    Err(e) => {
                        let _ = tx_port_token.send(Err(e.to_string()));
                        return;
                    }
                };
                let port = local_addr.port();

                let _ = tx_port_token.send(Ok(port));

                let state = Arc::new(WalletRpcState {
                    services,
                    token: token_clone,
                });

                let cors = CorsLayer::new()
                    .allow_methods([Method::POST, Method::GET])
                    .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION])
                    .allow_origin(tower_http::cors::AllowOrigin::predicate(|origin, _parts| {
                        let bytes = origin.as_bytes();
                        if bytes == b"tauri://localhost"
                            || bytes == b"http://localhost"
                            || bytes.starts_with(b"http://localhost:")
                            || bytes.starts_with(b"tauri://localhost:")
                        {
                            return true;
                        }

                        if let Ok(origin_str) = std::str::from_utf8(bytes) {
                            if let Ok(parsed_url) = url::Url::parse(origin_str) {
                                if let Some(host) = parsed_url.host_str() {
                                    return vaughan_trusted_hosts::hostname_is_whitelisted(host);
                                }
                            }
                        }
                        false
                    }));

                let app = Router::new()
                    .route("/health", get(health_handler))
                    .route(
                        "/rpc",
                        post(rpc_handler).route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware)),
                    )
                    .layer(cors)
                    .with_state(state);

                let _ = axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        let _ = rx_shutdown.await;
                    })
                    .await;
            });
        });

        let port = rx_port_token.recv()
            .map_err(|e| e.to_string())??;

        Ok((
            Self {
                tx_shutdown: std::sync::Mutex::new(Some(tx_shutdown)),
            },
            port,
            token,
        ))
    }
}

impl Drop for WalletRpcServer {
    fn drop(&mut self) {
        if let Ok(mut lock) = self.tx_shutdown.lock() {
            if let Some(tx) = lock.take() {
                let _ = tx.send(());
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(rename = "jsonrpc")]
    pub _jsonrpc: String,
    pub method: String,
    #[serde(default = "serde_json::Value::default")]
    pub params: serde_json::Value,
    pub id: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: serde_json::Value,
}

impl JsonRpcResponse {
    pub fn result(id: serde_json::Value, val: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(val),
            error: None,
            id,
        }
    }

    pub fn error(id: serde_json::Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
            id,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

fn constant_time_compare(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() != b_bytes.len() {
        return false;
    }
    let mut result = 0;
    for (x, y) in a_bytes.iter().zip(b_bytes.iter()) {
        result |= x ^ y;
    }
    result == 0
}

async fn auth_middleware(
    State(state): State<Arc<WalletRpcState>>,
    req: axum::extract::Request,
    next: middleware::Next,
) -> Result<impl IntoResponse, StatusCode> {
    let auth_header = req.headers().get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let authenticated = if let Some(auth) = auth_header {
        if auth.starts_with("Bearer ") {
            let provided_token = &auth["Bearer ".len()..];
            constant_time_compare(provided_token, &state.token)
        } else {
            false
        }
    } else {
        false
    };

    if authenticated {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn health_handler() -> impl IntoResponse {
    "OK"
}

fn deserialize_params<T: serde::de::DeserializeOwned>(params: serde_json::Value) -> Result<T, String> {
    if params.is_array() {
        let arr = params.as_array().ok_or("Params must be an array or object")?;
        if arr.is_empty() {
            return Err("Params array is empty".to_string());
        }
        serde_json::from_value(arr[0].clone()).map_err(|e| e.to_string())
    } else {
        serde_json::from_value(params).map_err(|e| e.to_string())
    }
}

async fn collect_accounts_for_rpc(services: &AppServices) -> Vec<AccountInfo> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    let from_mgr = services.account_manager.list_accounts().await;
    for a in from_mgr {
        let addr = address_to_hex(a.address);
        if seen.insert(addr.to_lowercase()) {
            out.push(AccountInfo {
                address: addr,
                name: Some(a.name),
            });
        }
    }

    let from_ws = services.wallet_state.accounts().await;
    for a in from_ws {
        let addr = address_to_hex(a.address);
        if seen.insert(addr.to_lowercase()) {
            out.push(AccountInfo {
                address: addr,
                name: Some(a.name),
            });
        }
    }

    out
}

async fn sign_message_response_async(
    services: &AppServices,
    payload: SignMessagePayload,
) -> Result<String, String> {
    let password = services.session_password().await
        .ok_or_else(|| "Wallet is locked: import/export password required before signing".to_string())?;

    let signer = load_signer_for_address(
        services.account_manager.as_ref(),
        &password,
        &payload.address,
    ).await.map_err(|e| e.user_message())?;

    let msg_bytes = if payload.message.starts_with("0x") {
        hex::decode(payload.message.trim_start_matches("0x"))
            .map_err(|_| "Invalid hex message payload".to_string())?
    } else {
        payload.message.into_bytes()
    };

    signer.sign_message_sync(&msg_bytes)
        .map(|sig| format!("{sig}"))
        .map_err(|err| format!("SignMessage failed: {err}"))
}

async fn sign_transaction_response_async(
    services: &AppServices,
    payload: SignTxPayload,
) -> Result<String, String> {
    let password = services.session_password().await
        .ok_or_else(|| "Wallet is locked: import/export password required before signing".to_string())?;

    let needle = payload.from.trim();
    let account = services.account_manager
        .list_accounts()
        .await
        .into_iter()
        .find(|a| address_to_hex(a.address).eq_ignore_ascii_case(needle))
        .ok_or_else(|| "Requested address not found in wallet accounts".to_string())?;

    let signer = load_signer_for_address(
        services.account_manager.as_ref(),
        &password,
        &payload.from,
    ).await.map_err(|e| e.user_message())?;

    if account.account_type == AccountType::SmartAccount {
        let active_network = services.network_service.active_network().await
            .ok_or_else(|| "No active network".to_string())?;
        let transport = alloy::transports::http::Http::new(url::Url::parse(&active_network.rpc_url).unwrap());
        let client = alloy::rpc::client::RpcClient::new(transport, true);
        let provider = alloy::providers::RootProvider::new(client);

        let is_deployed = is_account_deployed(account.address, &provider).await
            .map_err(|e| e.to_string())?;
        let info = account.smart_account.as_ref().unwrap();

        let inner_tx = AmbireAccount::Transaction {
            to: parse_address(&payload.to).map_err(|e| e.to_string())?,
            value: U256::from_str(&payload.value).map_err(|_| "Invalid wei amount".to_string())?,
            data: payload.data
                .as_ref()
                .map(|d| {
                    let stripped = d.trim_start_matches("0x");
                    hex::decode(stripped)
                        .map(alloy::primitives::Bytes::from)
                        .unwrap_or_default()
                })
                .unwrap_or_default(),
        };

        let calldata = if !is_deployed {
            let init_code = build_init_code(info.owner_address, AMBIRE_ACCOUNT_BYTECODE);
            build_signed_deploy_and_execute(
                &signer,
                account.address,
                init_code,
                info.salt,
                vec![inner_tx],
                payload.chain_id,
            )
            .await.map_err(|e| e.to_string())?
        } else {
            let nonce = get_smart_account_nonce(account.address, &provider).await
                .map_err(|e| e.to_string())?;
            build_signed_execute(
                &signer,
                account.address,
                vec![inner_tx],
                nonce,
                payload.chain_id,
            )
            .await.map_err(|e| e.to_string())?
        };

        let outer_to = if is_deployed { account.address } else { info.factory };
        let outer_tx = EvmTransaction {
            from: format!("{:?}", info.owner_address),
            to: format!("{:?}", outer_to),
            value: "0".into(),
            data: Some(format!("0x{}", hex::encode(&calldata))),
            gas_limit: parse_optional_u64_decimal(payload.gas_limit.as_deref()).unwrap_or_default(),
            gas_price: payload.gas_price,
            max_fee_per_gas: payload.max_fee_per_gas,
            max_priority_fee_per_gas: payload.max_priority_fee_per_gas,
            nonce: None,
            chain_id: payload.chain_id,
        };

        let tx_service = TransactionService::new();
        tx_service.sign_evm_transaction(&signer, &outer_tx).await
            .map_err(|e| format!("SignTransaction failed: {e}"))
    } else {
        let gas_limit = parse_optional_u64_decimal(payload.gas_limit.as_deref())
            .map_err(|e| e.user_message())?;
        let nonce = parse_optional_u64_decimal(payload.nonce.as_deref())
            .map_err(|e| e.user_message())?;

        let evm_tx = EvmTransaction {
            from: payload.from,
            to: payload.to,
            value: payload.value,
            data: payload.data,
            gas_limit,
            gas_price: payload.gas_price,
            max_fee_per_gas: payload.max_fee_per_gas,
            max_priority_fee_per_gas: payload.max_priority_fee_per_gas,
            nonce,
            chain_id: payload.chain_id,
        };

        let tx_service = TransactionService::new();
        tx_service.sign_evm_transaction(&signer, &evm_tx).await
            .map_err(|e| format!("SignTransaction failed: {e}"))
    }
}

async fn rpc_handler(
    State(state): State<Arc<WalletRpcState>>,
    Json(req): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let method = req.method.as_str();
    let result = match method {
        "vaughan_getNetworkInfo" => {
            let active = state.services.network_service.active_network_info().await;
            let info = match active {
                Some(n) => NetworkInfo {
                    chain_id: n.chain_id,
                    name: n.name,
                },
                None => NetworkInfo {
                    chain_id: 1,
                    name: "Ethereum".to_string(),
                },
            };
            Ok(serde_json::to_value(info).unwrap())
        }
        "vaughan_getAccounts" => {
            let accounts = collect_accounts_for_rpc(&state.services).await;
            Ok(serde_json::to_value(accounts).unwrap())
        }
        "vaughan_signMessage" => {
            let payload: SignMessagePayload = match deserialize_params(req.params) {
                Ok(p) => p,
                Err(e) => return Json(JsonRpcResponse::error(req.id, -32602, e)),
            };

            if let Err(ve) = payload.validate() {
                return Json(JsonRpcResponse::error(req.id, -32602, format!("Invalid payload: {ve}")));
            }

            let req_id_u64 = req.id.as_u64().unwrap_or(0);
            let payload_clone = payload.clone();
            let decision = tokio::task::spawn_blocking(move || {
                broker().submit_sign_message(req_id_u64, payload_clone, std::time::Duration::from_secs(120))
            }).await.unwrap_or(ApprovalDecision::Reject);

            match decision {
                ApprovalDecision::Approve => {
                    match sign_message_response_async(&state.services, payload).await {
                        Ok(sig) => Ok(serde_json::to_value(sig).unwrap()),
                        Err(e) => Err((4200, e)),
                    }
                }
                ApprovalDecision::Reject => {
                    Err((4001, "User rejected SignMessage request".to_string()))
                }
            }
        }
        "vaughan_signTransaction" => {
            let payload: SignTxPayload = match deserialize_params(req.params) {
                Ok(p) => p,
                Err(e) => return Json(JsonRpcResponse::error(req.id, -32602, e)),
            };

            if let Err(ve) = payload.validate() {
                return Json(JsonRpcResponse::error(req.id, -32602, format!("Invalid payload: {ve}")));
            }

            let req_id_u64 = req.id.as_u64().unwrap_or(0);
            let payload_clone = payload.clone();
            let decision = tokio::task::spawn_blocking(move || {
                broker().submit_sign_transaction(req_id_u64, payload_clone, std::time::Duration::from_secs(120))
            }).await.unwrap_or(ApprovalDecision::Reject);

            match decision {
                ApprovalDecision::Approve => {
                    match sign_transaction_response_async(&state.services, payload).await {
                        Ok(raw) => Ok(serde_json::Value::String(raw)),
                        Err(e) => Err((4200, e)),
                    }
                }
                ApprovalDecision::Reject => {
                    Err((4001, "User rejected SignTransaction request".to_string()))
                }
            }
        }
        "vaughan_addTrustedHost" => {
            let payload: AddTrustedHostPayload = match deserialize_params(req.params) {
                Ok(p) => p,
                Err(e) => return Json(JsonRpcResponse::error(req.id, -32602, e)),
            };

            if let Err(ve) = payload.validate() {
                return Json(JsonRpcResponse::error(req.id, -32602, format!("Invalid payload: {ve}")));
            }

            let name = payload.name.trim().to_string();
            let url_str = payload.url.trim().to_string();
            match url::Url::parse(&url_str) {
                Ok(parsed_url) => {
                    if let Some(host) = parsed_url.host_str() {
                        let host_lowercase = host.to_lowercase();
                        let scheme = parsed_url.scheme();
                        if scheme != "http" && scheme != "https" {
                            Err((4000, "Invalid scheme: only http/https allowed".to_string()))
                        } else if host_lowercase.contains('@') {
                            Err((4000, "Host cannot contain user credentials (@)".to_string()))
                        } else if scheme == "http" && host_lowercase != "localhost" && host_lowercase != "127.0.0.1" {
                            Err((4000, "HTTP protocol is only allowed for localhost/127.0.0.1".to_string()))
                        } else {
                            vaughan_trusted_hosts::add_custom_allowed_host(host_lowercase.clone());
                            let new_dapp = vaughan_core::core::persistence::CustomDapp {
                                name: name.clone(),
                                url: url_str.clone(),
                            };
                            let save_res = state.services.persistence.update_and_save(|st| {
                                if !st.custom_trusted_dapps.iter().any(|d| d.url == url_str) {
                                    st.custom_trusted_dapps.push(new_dapp);
                                }
                            }).await;

                            match save_res {
                                Ok(_) => {
                                    let snapshot = state.services.persistence.snapshot();
                                    let custom_info: Vec<CustomDappInfo> = snapshot.custom_trusted_dapps.into_iter().map(|d| {
                                        CustomDappInfo {
                                            name: d.name,
                                            url: d.url,
                                        }
                                    }).collect();
                                    Ok(serde_json::to_value(custom_info).unwrap())
                                }
                                Err(e) => Err((5000, format!("Failed to save custom host: {e}"))),
                            }
                        }
                    } else {
                        Err((4000, "URL does not contain a valid host".to_string()))
                    }
                }
                Err(e) => Err((4000, format!("Invalid URL: {e}"))),
            }
        }
        "vaughan_removeTrustedHost" => {
            #[derive(Deserialize)]
            struct RemovePayload {
                url: String,
            }
            let url_str: String = match deserialize_params::<RemovePayload>(req.params.clone()) {
                Ok(p) => p.url,
                Err(_) => {
                    match deserialize_params::<String>(req.params) {
                        Ok(s) => s,
                        Err(e) => return Json(JsonRpcResponse::error(req.id, -32602, e)),
                    }
                }
            };

            let host_to_remove = url::Url::parse(&url_str)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_lowercase()));

            if let Some(host) = host_to_remove {
                vaughan_trusted_hosts::remove_custom_allowed_host(&host);
            }

            let save_res = state.services.persistence.update_and_save(|st| {
                st.custom_trusted_dapps.retain(|d| d.url != url_str);
            }).await;

            match save_res {
                Ok(_) => {
                    let snapshot = state.services.persistence.snapshot();
                    let custom_info: Vec<CustomDappInfo> = snapshot.custom_trusted_dapps.into_iter().map(|d| {
                        CustomDappInfo {
                            name: d.name,
                            url: d.url,
                        }
                    }).collect();
                    Ok(serde_json::to_value(custom_info).unwrap())
                }
                Err(e) => Err((5000, format!("Failed to remove custom host: {e}"))),
            }
        }
        "vaughan_getTrustedHosts" => {
            let snapshot = state.services.persistence.snapshot();
            let custom_info: Vec<CustomDappInfo> = snapshot.custom_trusted_dapps.into_iter().map(|d| {
                CustomDappInfo {
                    name: d.name,
                    url: d.url,
                }
            }).collect();
            Ok(serde_json::to_value(custom_info).unwrap())
        }
        "vaughan_rpcForward" => {
            #[derive(Deserialize)]
            struct ForwardPayload {
                method: String,
                #[serde(default = "serde_json::Value::default")]
                params: serde_json::Value,
            }
            let payload: ForwardPayload = match deserialize_params(req.params) {
                Ok(p) => p,
                Err(e) => return Json(JsonRpcResponse::error(req.id, -32602, e)),
            };

            let rpc_url = match state.services.network_service.active_network().await {
                Some(net) => net.rpc_url,
                None => "https://rpc.pulsechain.com/".to_string(),
            };

            let client = reqwest::Client::new();
            let proxy_body = serde_json::json!({
                "jsonrpc": "2.0",
                "method": payload.method,
                "params": payload.params,
                "id": req.id
            });

            let res = match client.post(&rpc_url)
                .json(&proxy_body)
                .send()
                .await 
            {
                Ok(r) => r,
                Err(e) => return Json(JsonRpcResponse::error(req.id, -32603, format!("RPC forward failed: {e}"))),
            };

            if !res.status().is_success() {
                return Json(JsonRpcResponse::error(req.id, -32603, format!("RPC node returned error status: {}", res.status())));
            }

            let json_res: serde_json::Value = match res.json().await {
                Ok(j) => j,
                Err(e) => return Json(JsonRpcResponse::error(req.id, -32603, format!("Failed to parse RPC response: {e}"))),
            };

            if let Some(err) = json_res.get("error") {
                let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(-32603);
                let message = err.get("message").and_then(|m| m.as_str()).unwrap_or("RPC node error");
                return Json(JsonRpcResponse::error(req.id, code as i32, message.to_string()));
            }

            let result_val = json_res.get("result").cloned().unwrap_or(serde_json::Value::Null);
            Ok(result_val)
        }
        _ => Err((-32601, format!("Method '{method}' not found"))),
    };

    match result {
        Ok(val) => Json(JsonRpcResponse::result(req.id, val)),
        Err((code, msg)) => Json(JsonRpcResponse::error(req.id, code, msg)),
    }
}
