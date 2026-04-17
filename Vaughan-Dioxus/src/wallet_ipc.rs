use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use alloy::signers::local::PrivateKeySigner;
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
use vaughan_core::core::AccountType;
use vaughan_core::security::{derive_account, mnemonic_to_seed};

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
                    eprintln!("IPC server name parse failed");
                    return;
                };

                let listener = ListenerOptions::new().name(name).create_sync();
                let Ok(listener) = listener else {
                    eprintln!("IPC server bind failed: {}", endpoint);
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
                                eprintln!("IPC connection runtime init failed");
                                return;
                            };
                            handle_connection(stream, token.as_str(), &services, &rt);
                        })
                    {
                        eprintln!("IPC connection thread spawn failed: {e}");
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

/// Addresses exposed to dApps: `AccountManager` (imported / HD) plus `WalletState` (e.g. dashboard demo).
fn collect_accounts_for_ipc(
    services: &AppServices,
    runtime: &tokio::runtime::Runtime,
) -> Vec<AccountInfo> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();

    let from_mgr = runtime.block_on(services.account_manager.list_accounts());
    for a in from_mgr {
        let addr = format!("{:?}", a.address);
        if seen.insert(addr.to_lowercase()) {
            out.push(AccountInfo {
                address: addr,
                name: Some(a.name),
            });
        }
    }

    let from_ws = runtime.block_on(services.wallet_state.accounts());
    for a in from_ws {
        let addr = format!("{:?}", a.address);
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
                            IpcResponse::Error {
                                code: 4001,
                                message: "User rejected SignMessage request".to_string(),
                            }
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
                            IpcResponse::Error {
                                code: 4001,
                                message: "User rejected SignTransaction request".to_string(),
                            }
                        }
                    }
                }
                IpcRequest::SignTypedData(_) | IpcRequest::SwitchChain(_) => IpcResponse::Error {
                    code: 4200,
                    message: "Method not implemented in wallet yet".to_string(),
                },
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
        return IpcResponse::Error {
            code: 4100,
            message: "Wallet is locked: import/export password required before signing".to_string(),
        };
    };

    let signer = match load_signer_for_address(services, runtime, &payload.address, &password) {
        Ok(s) => s,
        Err(msg) => {
            return IpcResponse::Error {
                code: 4100,
                message: msg,
            };
        }
    };

    let msg_bytes = if payload.message.starts_with("0x") {
        match hex::decode(payload.message.trim_start_matches("0x")) {
            Ok(bytes) => bytes,
            Err(_) => {
                return IpcResponse::Error {
                    code: 4200,
                    message: "Invalid hex message payload".to_string(),
                };
            }
        }
    } else {
        payload.message.into_bytes()
    };

    match signer.sign_message_sync(&msg_bytes) {
        Ok(sig) => IpcResponse::SignedMessage(format!("{sig}")),
        Err(err) => IpcResponse::Error {
            code: 4200,
            message: format!("SignMessage failed: {err}"),
        },
    }
}

fn sign_transaction_response(
    services: &AppServices,
    runtime: &tokio::runtime::Runtime,
    payload: SignTxPayload,
) -> IpcResponse {
    let session_password = runtime.block_on(services.session_password());
    let Some(password) = session_password else {
        return IpcResponse::Error {
            code: 4100,
            message: "Wallet is locked: import/export password required before signing".to_string(),
        };
    };

    let signer = match load_signer_for_address(services, runtime, &payload.from, &password) {
        Ok(s) => s,
        Err(msg) => {
            return IpcResponse::Error {
                code: 4100,
                message: msg,
            };
        }
    };

    let gas_limit = match parse_optional_u64_decimal(payload.gas_limit.as_deref()) {
        Ok(v) => v,
        Err(msg) => {
            return IpcResponse::Error {
                code: 4200,
                message: msg,
            };
        }
    };
    let nonce = match parse_optional_u64_decimal(payload.nonce.as_deref()) {
        Ok(v) => v,
        Err(msg) => {
            return IpcResponse::Error {
                code: 4200,
                message: msg,
            };
        }
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
        Err(err) => IpcResponse::Error {
            code: 4200,
            message: format!("SignTransaction failed: {err}"),
        },
    }
}

fn load_signer_for_address(
    services: &AppServices,
    runtime: &tokio::runtime::Runtime,
    address: &str,
    password: &str,
) -> Result<PrivateKeySigner, String> {
    let accounts = runtime.block_on(services.account_manager.list_accounts());
    let account = accounts
        .into_iter()
        .find(|a| format!("{:?}", a.address).eq_ignore_ascii_case(address))
        .ok_or_else(|| "Requested address not found in wallet accounts".to_string())?;

    match account.account_type {
        AccountType::Imported => {
            let pk = services
                .account_manager
                .export_private_key(password, account.address)
                .map_err(|e| format!("Failed to load imported key: {e}"))?;
            PrivateKeySigner::from_str(pk.trim_start_matches("0x"))
                .map_err(|e| format!("Invalid private key material: {e}"))
        }
        AccountType::Hd => {
            let idx = account.index.unwrap_or(0);
            let mnemonic = services
                .account_manager
                .export_wallet_mnemonic(password)
                .map_err(|e| format!("Failed to load mnemonic from keyring: {e}"))?;
            let seed = mnemonic_to_seed(&mnemonic, None)
                .map_err(|e| format!("Failed to derive seed: {e}"))?;
            derive_account(&seed, idx).map_err(|e| format!("Failed to derive account key: {e}"))
        }
    }
}

fn parse_optional_u64_decimal(value: Option<&str>) -> Result<Option<u64>, String> {
    match value {
        None => Ok(None),
        Some(v) => {
            let t = v.trim();
            if t.is_empty() {
                return Ok(None);
            }
            t.parse::<u64>()
                .map(Some)
                .map_err(|_| format!("Invalid u64 decimal value: {t}"))
        }
    }
}
