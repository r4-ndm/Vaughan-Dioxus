//! Balance watcher: polls balances and emits changes.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::chains::{Balance, ChainAdapter};

#[derive(Debug, Clone)]
pub struct BalanceEvent {
    pub balance: Balance,
}

pub struct BalanceWatcher {
    stop_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl BalanceWatcher {
    /// Start polling `adapter.get_balance(address)` and send updates whenever it changes.
    pub fn start(
        adapter: Arc<dyn ChainAdapter>,
        address: String,
        interval: Duration,
        updates: mpsc::UnboundedSender<BalanceEvent>,
    ) -> Self {
        let (stop_tx, mut stop_rx) = oneshot::channel();

        let handle = tokio::spawn(async move {
            let mut last: Option<Balance> = None;
            let mut backoff = interval;
            let max_backoff = Duration::from_secs(60);

            loop {
                tokio::select! {
                    _ = &mut stop_rx => {
                        break;
                    }
                    _ = sleep(backoff) => {
                        match adapter.get_balance(&address).await {
                            Ok(bal) => {
                                backoff = interval; // reset after success
                                let changed = match &last {
                                    Some(prev) => prev.raw != bal.raw,
                                    None => true,
                                };
                                if changed {
                                    last = Some(bal.clone());
                                    let _ = updates.send(BalanceEvent { balance: bal });
                                }
                            }
                            Err(_) => {
                                // Exponential backoff on error (Task 12.5).
                                backoff = std::cmp::min(max_backoff, backoff.saturating_mul(2));
                            }
                        }
                    }
                }
            }
        });

        Self {
            stop_tx: Some(stop_tx),
            handle: Some(handle),
        }
    }

    /// Stop the polling task.
    pub async fn stop(mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}
