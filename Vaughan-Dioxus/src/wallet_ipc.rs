use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use alloy::signers::SignerSync;
use interprocess::local_socket::traits::Listener as _;
use interprocess::local_socket::{ListenerOptions, ToFsName};
use interprocess::TryClone;
use vaughan_ipc_types::{
    AccountInfo, Handshake, IpcEnvelope, IpcRequest, IpcResponse, NetworkInfo, SignMessagePayload,
    SignTxPayload, ValidationError, IPC_VERSION,
};

use crate::dapp_approval::{broker, ApprovalDecision};
use crate::services::AppServices;
use vaughan_core::chains::EvmTransaction;
use vaughan_core::core::transaction::TransactionService;
use vaughan_core::core::{address_to_hex, load_signer_for_address, parse_optional_u64_decimal, AccountType};
use vaughan_core::core::ambire_abi::AmbireAccount;
use vaughan_core::core::smart_account::{build_init_code, AMBIRE_ACCOUNT_BYTECODE};
use vaughan_core::core::scw_transaction::{
    build_signed_execute, build_signed_deploy_and_execute,
    get_smart_account_nonce, is_account_deployed,
};
use alloy::primitives::U256;
use vaughan_core::error::WalletError;
use vaughan_core::chains::evm::utils::parse_address;
use std::str::FromStr;

/// Set to true after a valid dApp-browser IPC handshake completes (see `handle_connection`).
/// Paired with a condvar so callers can wake as soon as the handshake completes instead of polling.
static DAPP_BROWSER_IPC_HANDSHAKE: Mutex<bool> = Mutex::new(false);
static DAPP_BROWSER_IPC_HANDSHAKE_CV: Condvar = Condvar::new();

#[inline]
pub fn reset_dapp_browser_ipc_handshake_gate() {
    if let Ok(mut g) = DAPP_BROWSER_IPC_HANDSHAKE.lock() {
        *g = false;
    }
}

/// Blocks until the dApp-browser IPC handshake completes or `max` elapses.
/// Used to defer work (e.g. hidden prewarm navigates) until the browser is ready.
#[inline]
pub fn wait_dapp_browser_ipc_handshake(max: Duration) -> bool {
    let deadline = Instant::now() + max;
    let mut g = DAPP_BROWSER_IPC_HANDSHAKE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    while !*g {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return *g;
        }
        let (guard, _) = DAPP_BROWSER_IPC_HANDSHAKE_CV.wait_timeout(g, left).unwrap();
        g = guard;
    }
    true
}

pub struct WalletIpcServer {
    stop: Arc<AtomicBool>,
    _worker: Option<thread::JoinHandle<()>>,
}

impl WalletIpcServer {
    pub fn start(
        endpoint: String,
        expected_token: String,
        services: AppServices,
    ) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = Arc::clone(&stop);

        let worker = thread::Builder::new()
            .name("vaughan-ipc-server".into())
            .spawn(move || {
                #[cfg(windows)]
                let name = endpoint
                    .clone()
                    .to_fs_name::<interprocess::os::windows::local_socket::NamedPipe>()
                    .map_err(|e| e.to_string());
                #[cfg(unix)]
                let name = endpoint
                    .clone()
                    .to_fs_name::<interprocess::os::unix::local_socket::FilesystemUdSocket>()
                    .map_err(|e| e.to_string());

                let Ok(name) = name else {
                    tracing::error!(target: "vaughan_ipc", "IPC server name parse failed");
                    return;
                };

                let listener = ListenerOptions::new().name(name).create_sync();
                let Ok(listener) = listener else {
                    tracing::error!(target: "vaughan_ipc", endpoint = %endpoint, "IPC server bind failed");
                    return;
                };

                while !stop_worker.load(Ordering::Relaxed) {
                    let conn = listener.accept();
                    let Ok(stream) = conn else {
                        continue;
                    };
                    let services = services.clone();
                    let token = expected_token.clone();
                    if let Err(e) = thread::Builder::new()
                        .name("vaughan-ipc-conn".into())
                        .spawn(move || {
                            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                            else {
                                tracing::error!(target: "vaughan_ipc", "IPC connection runtime init failed");
                                return;
                            };
                            handle_connection(stream, token.as_str(), &services, &rt);
                        })
                    {
                        tracing::error!(target: "vaughan_ipc", err = %e, "IPC connection thread spawn failed");
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(Self {
            stop,
            _worker: Some(worker),
        })
    }
}

impl Drop for WalletIpcServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn validation_error_message(e: ValidationError) -> String {
    format!("Invalid request: {e}")
}

fn ipc_error(code: u32, message: impl Into<String>) -> IpcResponse {
    IpcResponse::Error {
        code,
        message: message.into(),
    }
}

/// Addresses exposed to dApps: `AccountManager` (imported / HD) plus `WalletState` (e.g. dashboard demo).
fn collect_accounts_for_ipc(
    services: &AppServices,
    runtime: &tokio::runtime::Runtime,
) -> Vec<AccountInfo> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();

    let from_mgr = runtime.block_on(services.account_manager.list_accounts());
    for a in from_mgr {
        let addr = address_to_hex(a.address);
        if seen.insert(addr.to_lowercase()) {
            out.push(AccountInfo {
                address: addr,
                name: Some(a.name),
            });
        }
    }

    let from_ws = runtime.block_on(services.wallet_state.accounts());
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

fn handle_connection(
    mut stream: interprocess::local_socket::Stream,
    expected_token: &str,
    services: &AppServices,
    runtime: &tokio::runtime::Runtime,
) {
    let cloned = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = BufReader::new(cloned);

    let mut line = String::new();
    if reader.read_line(&mut line).ok().unwrap_or(0) == 0 {
        return;
    }

    let hs: Handshake = match serde_json::from_str(line.trim_end_matches(['\r', '\n'])) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(target: "vaughan_ipc", err = %e, "handshake JSON parse failed");
            return;
        }
    };
    if let Err(e) = hs.validate() {
        tracing::warn!(target: "vaughan_ipc", err = %e, "handshake validation failed");
        return;
    }
    if hs.version != IPC_VERSION {
        tracing::warn!(
            target: "vaughan_ipc",
            expected = IPC_VERSION,
            got = hs.version,
            "handshake IPC version mismatch"
        );
        return;
    }
    if hs.token != expected_token {
        tracing::warn!(target: "vaughan_ipc", "handshake token rejected");
        return;
    }

    let ack = match serde_json::to_string(&hs) {
        Ok(v) => v,
        Err(_) => return,
    };
    if stream.write_all(format!("{ack}\n").as_bytes()).is_err() {
        return;
    }
    {
        let mut done = DAPP_BROWSER_IPC_HANDSHAKE
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *done = true;
        DAPP_BROWSER_IPC_HANDSHAKE_CV.notify_all();
    }

    line.clear();
    loop {
        if reader.read_line(&mut line).ok().unwrap_or(0) == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let req: Result<IpcEnvelope<IpcRequest>, _> = serde_json::from_str(trimmed);
        if let Ok(req) = req {
            if let Err(ve) = req.body.validate() {
                tracing::warn!(
                    target: "vaughan_ipc",
                    request_id = req.id,
                    err = %ve,
                    "IPC request validation failed"
                );
                let resp = IpcEnvelope {
                    id: req.id,
                    body: IpcResponse::Error {
                        code: 32602,
                        message: validation_error_message(ve),
                    },
                };
                if let Ok(payload) = serde_json::to_string(&resp) {
                    if stream.write_all(format!("{payload}\n").as_bytes()).is_err() {
                        break;
                    }
                }
                line.clear();
                continue;
            }

            let body = match req.body {
                IpcRequest::GetAccounts => {
                    IpcResponse::Accounts(collect_accounts_for_ipc(services, runtime))
                }
                IpcRequest::GetNetworkInfo => {
                    let network = runtime.block_on(services.network_service.active_network_info());
                    match network {
                        Some(n) => IpcResponse::NetworkInfo(NetworkInfo {
                            chain_id: n.chain_id,
                            name: n.name,
                        }),
                        None => IpcResponse::NetworkInfo(NetworkInfo {
                            chain_id: 1,
                            name: "Ethereum".to_string(),
                        }),
                    }
                }
                IpcRequest::SignMessage(payload) => {
                    let payload_for_sign = payload.clone();
                    let decision = broker().submit_sign_message(
                        req.id,
                        payload,
                        std::time::Duration::from_secs(120),
                    );
                    match decision {
                        ApprovalDecision::Approve => {
                            tracing::info!(target: "vaughan_ipc", request_id = req.id, "SignMessage approved");
                            sign_message_response(services, runtime, payload_for_sign)
                        }
                        ApprovalDecision::Reject => {
                            tracing::info!(target: "vaughan_ipc", request_id = req.id, "SignMessage rejected by user");
                            ipc_error(4001, "User rejected SignMessage request")
                        }
                    }
                }
                IpcRequest::SignTransaction(payload) => {
                    let payload_for_sign = payload.clone();
                    let decision = broker().submit_sign_transaction(
                        req.id,
                        payload,
                        std::time::Duration::from_secs(120),
                    );
                    match decision {
                        ApprovalDecision::Approve => {
                            tracing::info!(target: "vaughan_ipc", request_id = req.id, "SignTransaction approved");
                            sign_transaction_response(services, runtime, payload_for_sign)
                        }
                        ApprovalDecision::Reject => {
                            tracing::info!(target: "vaughan_ipc", request_id = req.id, "SignTransaction rejected by user");
                            ipc_error(4001, "User rejected SignTransaction request")
                        }
                    }
                }
                IpcRequest::SignTypedData(_) => ipc_error(4200, "Method not implemented in wallet yet"),
                IpcRequest::SwitchChain(payload) => {
                    let chain_id = payload.chain_id;
                    let networks = runtime.block_on(services.network_service.list_networks());
                    if let Some(net) = networks.into_iter().find(|n| n.chain_id == chain_id) {
                        if let Err(e) =
                            runtime.block_on(services.network_service.set_active_network(&net.id))
                        {
                            ipc_error(4200, e.to_string())
                        } else {
                            let id = net.id.clone();
                            if let Err(e) = runtime.block_on(services.persistence.update_and_save(
                                |st| {
                                    st.active_network_id = Some(id);
                                },
                            )) {
                                tracing::warn!(
                                    target: "vaughan_ipc",
                                    err = %e,
                                    "SwitchChain: failed to persist active network"
                                );
                            }
                            IpcResponse::NetworkInfo(NetworkInfo {
                                chain_id: net.chain_id,
                                name: net.name,
                            })
                        }
                    } else {
                        ipc_error(
                            4902,
                            format!(
                                "Chain id {chain_id} is not in Vaughan's network list. Add it in Settings, then retry."
                            ),
                        )
                    }
                }
            };

            let resp = IpcEnvelope { id: req.id, body };
            if let Ok(payload) = serde_json::to_string(&resp) {
                if stream.write_all(format!("{payload}\n").as_bytes()).is_err() {
                    break;
                }
            }
        }
        line.clear();
    }
}

fn sign_message_response(
    services: &AppServices,
    runtime: &tokio::runtime::Runtime,
    payload: SignMessagePayload,
) -> IpcResponse {
    let session_password = runtime.block_on(services.session_password());
    let Some(password) = session_password else {
        return ipc_error(
            4100,
            "Wallet is locked: import/export password required before signing",
        );
    };

    let signer = match runtime.block_on(load_signer_for_address(
        services.account_manager.as_ref(),
        &password,
        &payload.address,
    )) {
        Ok(s) => s,
        Err(e) => return ipc_error(4100, e.user_message()),
    };

    let msg_bytes = if payload.message.starts_with("0x") {
        match hex::decode(payload.message.trim_start_matches("0x")) {
            Ok(bytes) => bytes,
            Err(_) => {
                return ipc_error(4200, "Invalid hex message payload");
            }
        }
    } else {
        payload.message.into_bytes()
    };

    match signer.sign_message_sync(&msg_bytes) {
        Ok(sig) => IpcResponse::SignedMessage(format!("{sig}")),
        Err(err) => ipc_error(4200, format!("SignMessage failed: {err}")),
    }
}

fn sign_transaction_response(
    services: &AppServices,
    runtime: &tokio::runtime::Runtime,
    payload: SignTxPayload,
) -> IpcResponse {
    let session_password = runtime.block_on(services.session_password());
    let Some(password) = session_password else {
        return ipc_error(
            4100,
            "Wallet is locked: import/export password required before signing",
        );
    };

    let needle = payload.from.trim();
    let account = match runtime.block_on(async {
        services.account_manager
            .list_accounts()
            .await
            .into_iter()
            .find(|a| address_to_hex(a.address).eq_ignore_ascii_case(needle))
            .ok_or_else(|| {
                WalletError::AccountNotFound("Requested address not found in wallet accounts".into())
            })
    }) {
        Ok(a) => a,
        Err(e) => return ipc_error(4100, e.user_message()),
    };

    let signer = match runtime.block_on(load_signer_for_address(
        services.account_manager.as_ref(),
        &password,
        &payload.from,
    )) {
        Ok(s) => s,
        Err(e) => return ipc_error(4100, e.user_message()),
    };

    if account.account_type == AccountType::SmartAccount {
        let result = runtime.block_on(async {
            let active_network = services.network_service.active_network().await
                .ok_or_else(|| WalletError::UnsupportedChain("No active network".into()))?;
            let transport = alloy::transports::http::Http::new(url::Url::parse(&active_network.rpc_url).unwrap());
            let client = alloy::rpc::client::RpcClient::new(transport, true);
            let provider = alloy::providers::RootProvider::new(client);

            let is_deployed = is_account_deployed(account.address, &provider).await?;
            let info = account.smart_account.as_ref().unwrap();

            let inner_tx = AmbireAccount::Transaction {
                to: parse_address(&payload.to)?,
                value: U256::from_str(&payload.value).map_err(|_| WalletError::InvalidAmount("Invalid wei amount".into()))?,
                data: payload.data
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
                .await?
            } else {
                let nonce = get_smart_account_nonce(account.address, &provider).await?;
                build_signed_execute(
                    &signer,
                    account.address,
                    vec![inner_tx],
                    nonce,
                    payload.chain_id,
                )
                .await?
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
        });

        match result {
            Ok(raw) => IpcResponse::SignedTransaction(raw),
            Err(err) => ipc_error(4200, format!("SignTransaction failed: {err}")),
        }
    } else {
        let gas_limit = match parse_optional_u64_decimal(payload.gas_limit.as_deref()) {
            Ok(v) => v,
            Err(e) => return ipc_error(4200, e.user_message()),
        };
        let nonce = match parse_optional_u64_decimal(payload.nonce.as_deref()) {
            Ok(v) => v,
            Err(e) => return ipc_error(4200, e.user_message()),
        };

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
        match runtime.block_on(tx_service.sign_evm_transaction(&signer, &evm_tx)) {
            Ok(raw) => IpcResponse::SignedTransaction(raw),
            Err(err) => ipc_error(4200, format!("SignTransaction failed: {err}")),
        }
    }
}
