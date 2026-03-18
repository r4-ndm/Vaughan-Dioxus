//! Vaughan dApp browser library.
//!
//! Task 30.3: IPC client bootstrap (connect + handshake).

mod ipc;

use std::time::Duration;

use ipc::IpcClient;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use tokio::sync::{mpsc, oneshot};
use vaughan_ipc_types::{IpcRequest, IpcResponse};

#[derive(Debug)]
struct PendingIpcRequest {
    id: u64,
    req: IpcRequest,
    timeout: Duration,
    resp: oneshot::Sender<IpcResponse>,
}

#[derive(Clone)]
struct IpcForwarderHandle {
    tx: mpsc::Sender<PendingIpcRequest>,
    next_id: Arc<AtomicU64>,
}

impl IpcForwarderHandle {
    fn next(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
struct CliConfig {
    ipc_endpoint: String,
    token: String,
}

fn parse_args() -> Option<CliConfig> {
    let mut ipc = None::<String>;
    let mut token = None::<String>;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--ipc" | "--ipc-endpoint" => ipc = it.next(),
            "--token" => token = it.next(),
            _ => {}
        }
    }

    Some(CliConfig {
        ipc_endpoint: ipc?,
        token: token?,
    })
}

pub fn run() {
    let cli = parse_args();

    // Channel between Tauri commands (async invoke) and a single background IPC task.
    let (req_tx, req_rx) = mpsc::channel::<PendingIpcRequest>(32);
    let forwarder = IpcForwarderHandle {
        tx: req_tx,
        next_id: Arc::new(AtomicU64::new(1)),
    };

    tauri::Builder::default()
        .manage(forwarder.clone())
        .invoke_handler(tauri::generate_handler![ipc_request])
        .setup(move |_app| {
            if let Some(cli) = cli.clone() {
                let mut rx = req_rx;
                tokio::spawn(async move {
                    loop {
                        let connect_timeout = Duration::from_secs(3);
                        let mut client =
                            match IpcClient::connect(&cli.ipc_endpoint, &cli.token, connect_timeout).await
                            {
                                Ok(c) => c,
                                Err(e) => {
                                    eprintln!("IPC connect failed: {e}");
                                    tokio::time::sleep(Duration::from_secs(2)).await;
                                    continue;
                                }
                            };

                        eprintln!("IPC connected: {}", cli.ipc_endpoint);

                        // Serve requests until the channel closes or the connection errors.
                        while let Some(pending) = rx.recv().await {
                            let resp = match client.request(
                                pending.id,
                                pending.req,
                                pending.timeout,
                            ).await {
                                Ok(env) => env.body,
                                Err(_e) => IpcResponse::Error {
                                    code: 1000,
                                    message: "IPC request failed".into(),
                                },
                            };
                            let _ = pending.resp.send(resp);
                        }

                        // Channel is closed -> stop background task.
                        break;
                    }
                });
            } else {
                eprintln!("IPC config missing. Start with --ipc <endpoint> --token <token>");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}

/// Generic IPC forwarder command.
#[tauri::command]
async fn ipc_request(
    forwarder: tauri::State<'_, IpcForwarderHandle>,
    request: IpcRequest,
) -> Result<IpcResponse, String> {
    let (resp_tx, resp_rx) = oneshot::channel::<IpcResponse>();

    let pending = PendingIpcRequest {
        id: forwarder.next(),
        req: request,
        timeout: Duration::from_secs(10),
        resp: resp_tx,
    };

    let tx = forwarder.tx.clone();
    tx.send(pending).await.map_err(|_send_err| "IPC forwarder unavailable".to_string())?;

    match tokio::time::timeout(Duration::from_secs(12), resp_rx).await {
        Ok(Ok(resp)) => Ok(resp),
        _ => Err("IPC request timed out".to_string()),
    }
}
