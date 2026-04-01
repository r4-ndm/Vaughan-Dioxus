use std::collections::HashMap;
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;

use vaughan_ipc_types::{SignMessagePayload, SignTxPayload};

#[derive(Clone, Debug)]
pub struct PendingSignMessage {
    pub request_id: u64,
    pub payload: SignMessagePayload,
}

#[derive(Clone, Debug)]
pub struct PendingSignTransaction {
    pub request_id: u64,
    pub payload: SignTxPayload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approve,
    Reject,
}

pub struct DappApprovalBroker {
    pending_sign_message: Mutex<Option<PendingSignMessage>>,
    sign_message_waiters: Mutex<HashMap<u64, mpsc::SyncSender<ApprovalDecision>>>,
    pending_sign_transaction: Mutex<Option<PendingSignTransaction>>,
    sign_transaction_waiters: Mutex<HashMap<u64, mpsc::SyncSender<ApprovalDecision>>>,
}

impl DappApprovalBroker {
    fn new() -> Self {
        Self {
            pending_sign_message: Mutex::new(None),
            sign_message_waiters: Mutex::new(HashMap::new()),
            pending_sign_transaction: Mutex::new(None),
            sign_transaction_waiters: Mutex::new(HashMap::new()),
        }
    }

    pub fn submit_sign_message(
        &self,
        request_id: u64,
        payload: SignMessagePayload,
        timeout: Duration,
    ) -> ApprovalDecision {
        let (tx, rx) = mpsc::sync_channel::<ApprovalDecision>(1);

        if let Ok(mut waiters) = self.sign_message_waiters.lock() {
            waiters.insert(request_id, tx);
        }
        if let Ok(mut pending) = self.pending_sign_message.lock() {
            *pending = Some(PendingSignMessage {
                request_id,
                payload,
            });
        }

        let decision = rx.recv_timeout(timeout).unwrap_or(ApprovalDecision::Reject);
        self.clear_sign_message(request_id);
        decision
    }

    pub fn pending_sign_message(&self) -> Option<PendingSignMessage> {
        self.pending_sign_message
            .lock()
            .ok()
            .and_then(|p| p.clone())
    }

    pub fn submit_sign_transaction(
        &self,
        request_id: u64,
        payload: SignTxPayload,
        timeout: Duration,
    ) -> ApprovalDecision {
        let (tx, rx) = mpsc::sync_channel::<ApprovalDecision>(1);

        if let Ok(mut waiters) = self.sign_transaction_waiters.lock() {
            waiters.insert(request_id, tx);
        }
        if let Ok(mut pending) = self.pending_sign_transaction.lock() {
            *pending = Some(PendingSignTransaction {
                request_id,
                payload,
            });
        }

        let decision = rx.recv_timeout(timeout).unwrap_or(ApprovalDecision::Reject);
        self.clear_sign_transaction(request_id);
        decision
    }

    pub fn pending_sign_transaction(&self) -> Option<PendingSignTransaction> {
        self.pending_sign_transaction
            .lock()
            .ok()
            .and_then(|p| p.clone())
    }

    pub fn approve_sign_message(&self, request_id: u64) -> bool {
        self.resolve_sign_message(request_id, ApprovalDecision::Approve)
    }

    pub fn reject_sign_message(&self, request_id: u64) -> bool {
        self.resolve_sign_message(request_id, ApprovalDecision::Reject)
    }

    pub fn approve_sign_transaction(&self, request_id: u64) -> bool {
        self.resolve_sign_transaction(request_id, ApprovalDecision::Approve)
    }

    pub fn reject_sign_transaction(&self, request_id: u64) -> bool {
        self.resolve_sign_transaction(request_id, ApprovalDecision::Reject)
    }

    fn resolve_sign_message(&self, request_id: u64, decision: ApprovalDecision) -> bool {
        let sender = self
            .sign_message_waiters
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&request_id));
        if let Some(tx) = sender {
            let _ = tx.send(decision);
            true
        } else {
            false
        }
    }

    fn clear_sign_message(&self, request_id: u64) {
        if let Ok(mut waiters) = self.sign_message_waiters.lock() {
            waiters.remove(&request_id);
        }
        if let Ok(mut pending) = self.pending_sign_message.lock() {
            let should_clear = pending.as_ref().map(|p| p.request_id) == Some(request_id);
            if should_clear {
                *pending = None;
            }
        }
    }

    fn resolve_sign_transaction(&self, request_id: u64, decision: ApprovalDecision) -> bool {
        let sender = self
            .sign_transaction_waiters
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&request_id));
        if let Some(tx) = sender {
            let _ = tx.send(decision);
            true
        } else {
            false
        }
    }

    fn clear_sign_transaction(&self, request_id: u64) {
        if let Ok(mut waiters) = self.sign_transaction_waiters.lock() {
            waiters.remove(&request_id);
        }
        if let Ok(mut pending) = self.pending_sign_transaction.lock() {
            let should_clear = pending.as_ref().map(|p| p.request_id) == Some(request_id);
            if should_clear {
                *pending = None;
            }
        }
    }
}

static BROKER: OnceLock<DappApprovalBroker> = OnceLock::new();

pub fn broker() -> &'static DappApprovalBroker {
    BROKER.get_or_init(DappApprovalBroker::new)
}
