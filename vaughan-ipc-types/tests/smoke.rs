//! Integration tests under `tests/` for tooling that keys off this layout.

use vaughan_ipc_types::{Handshake, IPC_VERSION, IpcEnvelope, IpcRequest};

#[test]
fn handshake_and_envelope_serde_smoke() {
    let h = Handshake {
        version: IPC_VERSION,
        token: "smoke-token".into(),
    };
    h.validate().expect("handshake");

    let req = IpcEnvelope {
        id: 42,
        body: IpcRequest::GetAccounts,
    };
    let json = serde_json::to_string(&req).expect("serialize");
    let back: IpcEnvelope<IpcRequest> = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.id, 42);
    assert!(matches!(back.body, IpcRequest::GetAccounts));
}
