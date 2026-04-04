//! Persistent local-socket pool to the Dioxus wallet: one connect+handshake per slot,
//! not per RPC (see workspace plan: persistent IPC).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use vaughan_ipc_types::{IpcEnvelope, IpcRequest, IpcResponse};

use crate::ipc::{IpcClient, IpcClientError};

const DEFAULT_POOL_SIZE: usize = 4;

pub struct WalletIpcPool {
    endpoint: String,
    token: String,
    connect_timeout: Duration,
    slots: Vec<Mutex<Option<IpcClient>>>,
    next_slot: AtomicUsize,
}

impl WalletIpcPool {
    pub fn new(endpoint: String, token: String, connect_timeout: Duration) -> Arc<Self> {
        let size = std::env::var("VAUGHAN_IPC_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n: &usize| n > 0 && n <= 32)
            .unwrap_or(DEFAULT_POOL_SIZE);
        let slots = (0..size).map(|_| Mutex::new(None)).collect();
        Arc::new(Self {
            endpoint,
            token,
            connect_timeout,
            slots,
            next_slot: AtomicUsize::new(0),
        })
    }

    pub async fn request(
        &self,
        id: u64,
        body: IpcRequest,
        op_timeout: Duration,
    ) -> Result<IpcEnvelope<IpcResponse>, String> {
        let t_total = Instant::now();
        let n = self.slots.len();
        let start = self.next_slot.fetch_add(1, Ordering::Relaxed) % n;

        let mut last_connect_err: Option<String> = None;

        for round in 0..(n * 2) {
            let i = (start + round) % n;
            let mut guard = self.slots[i].lock().await;

            let reused = guard.is_some();
            if !reused {
                let t_conn = Instant::now();
                match IpcClient::connect(&self.endpoint, &self.token, self.connect_timeout).await {
                    Ok(c) => {
                        let ms = t_conn.elapsed().as_secs_f64() * 1000.0;
                        tracing::debug!(
                            target: "vaughan_ipc_browser",
                            slot = i,
                            connect_ms = ms,
                            "ipc pool new connection"
                        );
                        *guard = Some(c);
                    }
                    Err(e) => {
                        last_connect_err = Some(e.to_string());
                        continue;
                    }
                }
            }

            let client = guard.as_mut().unwrap();
            let t_req = Instant::now();
            let result = client.request(id, body.clone(), op_timeout).await;
            let request_ms = t_req.elapsed().as_secs_f64() * 1000.0;

            match result {
                Ok(env) => {
                    let total_ms = t_total.elapsed().as_secs_f64() * 1000.0;
                    tracing::debug!(
                        target: "vaughan_ipc_browser",
                        slot = i,
                        reused,
                        request_ms,
                        total_ms,
                        "ipc pool roundtrip ok"
                    );
                    return Ok(env);
                }
                Err(e) => {
                    if should_drop_connection(&e) {
                        tracing::debug!(
                            target: "vaughan_ipc_browser",
                            slot = i,
                            err = %e,
                            "ipc pool dropping dead connection"
                        );
                        *guard = None;
                        continue;
                    }
                    return Err(format!("IPC request failed: {e}"));
                }
            }
        }

        Err(last_connect_err
            .unwrap_or_else(|| "IPC connect failed: exhausted pool retries".to_string()))
    }

    /// Eagerly connect idle pool slots so the first dApp RPC avoids paying connect+handshake latency
    /// on the critical path (runs in the background right after the webview is created).
    pub async fn warm_connections(&self) {
        for (i, slot) in self.slots.iter().enumerate() {
            let mut guard = slot.lock().await;
            if guard.is_some() {
                continue;
            }
            match IpcClient::connect(&self.endpoint, &self.token, self.connect_timeout).await {
                Ok(c) => {
                    tracing::debug!(
                        target: "vaughan_ipc_browser",
                        slot = i,
                        "ipc pool warm connection ready"
                    );
                    *guard = Some(c);
                }
                Err(e) => {
                    tracing::debug!(
                        target: "vaughan_ipc_browser",
                        slot = i,
                        err = %e,
                        "ipc pool warm connection skipped"
                    );
                }
            }
        }
    }
}

fn should_drop_connection(e: &IpcClientError) -> bool {
    matches!(
        e,
        IpcClientError::Io(_) | IpcClientError::UnexpectedResponse | IpcClientError::Serde(_)
    )
}
