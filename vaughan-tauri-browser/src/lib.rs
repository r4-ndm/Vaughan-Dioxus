//! Vaughan dApp browser library.
//!
//! Task 30.3: IPC client bootstrap (connect + handshake).

mod ipc;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ipc::IpcClient;
use tauri::WebviewUrl;

use vaughan_ipc_types::{IpcRequest, IpcResponse};

#[derive(Debug, Clone)]
struct CliConfig {
    ipc_endpoint: String,
    token: String,
    initial_url: Option<String>,
}

/// Endpoint + token for per-request IPC connections (parallel dApp RPC).
#[derive(Clone)]
struct WalletIpcConnectInfo {
    endpoint: String,
    token: String,
}

static IPC_REQ_ID: AtomicU64 = AtomicU64::new(1);

fn parse_args() -> Option<CliConfig> {
    let mut ipc = None::<String>;
    let mut token = None::<String>;
    let mut initial_url = None::<String>;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--ipc" | "--ipc-endpoint" => ipc = it.next(),
            "--token" => token = it.next(),
            "--url" => initial_url = it.next(),
            _ => {}
        }
    }

    Some(CliConfig {
        ipc_endpoint: ipc?,
        token: token?,
        initial_url,
    })
}

pub fn run() {
    let cli = parse_args();

    let ipc_connect = cli.as_ref().map(|c| WalletIpcConnectInfo {
        endpoint: c.ipc_endpoint.clone(),
        token: c.token.clone(),
    });

    let navigate_after_load = cli
        .as_ref()
        .and_then(|c| c.initial_url.clone())
        .filter(|u| !u.trim().is_empty());

    tauri::Builder::default()
        .manage(ipc_connect)
        .invoke_handler(tauri::generate_handler![ipc_request])
        .setup(move |app| {
            const PROVIDER_INIT: &str = include_str!("../provider_inject.js");
            let pending_js = navigate_after_load.as_ref().map(|url| {
                format!(
                    "window.__VAUGHAN_PENDING_INITIAL_URL={};",
                    serde_json::to_string(url).unwrap_or_else(|_| "\"\"".into())
                )
            });

            let mut wv =
                tauri::WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("Vaughan - dApp Browser")
                    .inner_size(1200.0, 800.0)
                    .resizable(true)
                    .initialization_script_for_all_frames(PROVIDER_INIT);

            if let Some(script) = pending_js.as_ref() {
                wv = wv.initialization_script(script);
            }

            wv.build()
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            if cli.is_none() {
                eprintln!("IPC config missing. Start with --ipc <endpoint> --token <token>");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}

/// Each invoke opens a short-lived IPC connection so parallel dApp RPCs do not queue
/// behind a single socket (Uniswap/wagmi issue many concurrent `eth_*` calls).
#[tauri::command]
async fn ipc_request(
    cfg: tauri::State<'_, Option<WalletIpcConnectInfo>>,
    request: IpcRequest,
) -> Result<IpcResponse, String> {
    let Some(info) = cfg.as_ref() else {
        return Err("Wallet IPC not configured".to_string());
    };

    let connect_timeout = Duration::from_secs(3);
    let op_timeout = Duration::from_secs(10);
    let mut client = IpcClient::connect(&info.endpoint, &info.token, connect_timeout)
        .await
        .map_err(|e| format!("IPC connect failed: {e}"))?;

    let id = IPC_REQ_ID.fetch_add(1, Ordering::Relaxed);
    let env = client
        .request(id, request, op_timeout)
        .await
        .map_err(|e| format!("IPC request failed: {e}"))?;

    Ok(env.body)
}
