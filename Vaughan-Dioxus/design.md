# Design Document: Dioxus + Tauri Multi-Chain Wallet

## Overview

This document details the design for building a new Dioxus-based multi-chain cryptocurrency wallet from scratch. The wallet core (chain adapters, security, state management, transaction logic) is identical to the Iced-based design. Only the GUI layer and dApp browser integration differ: Dioxus replaces Iced for the wallet UI (enabling desktop + mobile), and a Tauri + Dioxus application replaces the pure Dioxus browser for the dApp browser (providing OS-level process isolation via Tauri's WebView).

### Design Philosophy

1. **Vaughan-Tauri as workflow authority**: **[Vaughan-Tauri](https://github.com/r4-ndm/Vaughan-Tauri)** is the primary reference for tested flows, Alloy usage, EIP-1193 behavior, and security patterns. This Dioxus stack should match it unless we document an intentional difference (see repo root [`REFERENCE-Vaughan-Tauri.md`](../REFERENCE-Vaughan-Tauri.md)).
2. **Greenfield UI**: New Dioxus presentation layer; Vaughan-Iced remains a secondary historical reference where it does not conflict with Vaughan-Tauri.
3. **Shared Core**: Chain adapters, security, and business logic follow the same boundaries as the reference designs — only the GUI and process layout differ.
4. **Cross-Platform First**: Dioxus enables a single codebase for desktop (Windows, macOS, Linux) and mobile (Android, iOS).
5. **Security by Design**: Security-critical functionality isolated in dedicated modules using only audited cryptographic libraries.
6. **Process Isolation for dApps**: The Tauri dApp browser runs as a separate OS process, ensuring dApp code never touches wallet keys.
7. **Lightweight**: Aggressive dependency management to minimize binary size and attack surface.

### Key Design Decisions

- **Dioxus for GUI**: Component-based reactive UI targeting desktop and mobile from a single codebase
- **Tauri for dApp Browser**: Proven WebView host with OS-level process isolation and native IPC
- **Shared Core**: `vaughan-core` mirrors Vaughan-Tauri’s `src-tauri` chains/core/security split; treat [Vaughan-Tauri](https://github.com/r4-ndm/Vaughan-Tauri) as the behavioral reference, Iced docs as secondary
- **interprocess for IPC**: Cross-platform IPC abstraction (Unix domain sockets on Linux/macOS, named pipes on Windows)
- **Shared Theme**: Same visual identity applied to both Dioxus wallet and Tauri browser
- **Feature Flags**: audio, qr, hardware-wallets, telemetry all optional

## Architecture

### Layered Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Layer 2: GUI (Dioxus)                                  │
│  - Component-based reactive UI                          │
│  - use_signal / use_context state management            │
│  - use_coroutine for async backend operations           │
│  - Desktop (dioxus-desktop) + Mobile (dioxus-mobile)    │
└─────────────────────────────────────────────────────────┘
                         ↓ ↑
┌─────────────────────────────────────────────────────────┐
│  Layer 1: Core (Chain-Agnostic Business Logic)          │
│  - WalletState (adapter coordination)                   │
│  - Account management                                   │
│  - Transaction logic                                    │
│  - Network management                                   │
│  - Persistence                                          │
└─────────────────────────────────────────────────────────┘
                         ↓ ↑
┌─────────────────────────────────────────────────────────┐
│  Layer 0: Chain Adapters (Chain-Specific Operations)    │
│  - ChainAdapter trait                                   │
│  - EVM adapter (Alloy)                                  │
│  - Future: Stellar, Aptos, Solana, Bitcoin              │
└─────────────────────────────────────────────────────────┘
```

### Process Architecture

```
┌─────────────────────────────────────┐      ┌─────────────────────────────────────┐
│     Dioxus Wallet (Parent)          │      │    Tauri dApp Browser (Child)       │
│                                     │      │                                     │
│  ┌─────────────────────────────┐    │      │  ┌─────────────────────────────┐    │
│  │   vaughan-core (Rust)       │    │      │  │   Tauri WebView             │    │
│  │  Keys, signing, accounts    │    │      │  │   Loads arbitrary dApps     │    │
│  └──────────────┬──────────────┘    │      │  └──────────────┬──────────────┘    │
│                 │                    │      │                 │                    │
│  ┌──────────────▼──────────────┐    │      │  ┌──────────────▼──────────────┐    │
│  │   IPC Server (interprocess)   │◄───┼──────┼──│   IPC Client (interprocess)   │    │
│  │   Listens for requests      │    │  IPC │  │   Sends signing requests    │    │
│  └─────────────────────────────┘    │      │  └─────────────────────────────┘    │
│                                     │      │                                     │
│  ┌─────────────────────────────┐    │      │  ┌─────────────────────────────┐    │
│  │   Dioxus UI (Native)        │    │      │  │   Dioxus UI (inside Tauri)  │    │
│  │   Dashboard, settings       │    │      │  │   Address bar, nav buttons  │    │
│  └─────────────────────────────┘    │      │  └─────────────────────────────┘    │
└─────────────────────────────────────┘      └─────────────────────────────────────┘
```

### Workspace Structure

```
vaughan-dioxus-tauri/
├── vaughan-core/               # Shared: chains, core, security, monitoring, audio
│   └── src/
│       ├── chains/             # Layer 0: ChainAdapter trait + EVM adapter
│       ├── core/               # Layer 1: WalletState, accounts, transactions
│       ├── security/           # Keyring, encryption, HD wallet
│       ├── monitoring/         # Balance watcher
│       ├── audio/              # Sound notifications (optional)
│       ├── models/             # Shared types
│       └── error/              # Error types
├── vaughan-dioxus/             # Layer 2: Dioxus wallet GUI
│   └── src/
│       ├── gui/
│       │   ├── app.rs          # Root WalletApp component
│       │   ├── state.rs        # Global state (use_context)
│       │   ├── views/          # Dashboard, Send, Receive, History, Settings, Import
│       │   ├── widgets/        # Reusable components
│       │   └── theme/          # Colors, fonts, spacing
│       └── main.rs             # Desktop entry point
├── vaughan-tauri-browser/      # Tauri dApp browser
│   ├── src/
│   │   ├── main.rs             # Tauri app entry point
│   │   ├── ipc_client.rs       # IPC connection to wallet
│   │   └── provider.js         # EIP-1193/6963 injection script
│   ├── src-dioxus/             # Dioxus UI inside Tauri
│   │   └── browser_ui.rs       # Address bar, nav, status
│   └── tauri.conf.json
├── vaughan-ipc-types/          # Shared IPC message types
│   └── src/lib.rs
└── Cargo.toml                  # Workspace root
```

## Components and Interfaces

### 1. Chain Adapter Layer (Layer 0) — Identical to Iced Design

The `ChainAdapter` trait and EVM adapter are reused verbatim. No changes from the Iced design.

```rust
#[async_trait]
pub trait ChainAdapter: Send + Sync {
    async fn get_balance(&self, address: &str) -> Result<Balance, WalletError>;
    async fn get_token_balance(&self, token_address: &str, wallet_address: &str)
        -> Result<Balance, WalletError>;
    async fn send_transaction(&self, tx: ChainTransaction) -> Result<TxHash, WalletError>;
    async fn sign_message(&self, address: &str, message: &[u8]) -> Result<Signature, WalletError>;
    async fn get_transactions(&self, address: &str, limit: u32) -> Result<Vec<TxRecord>, WalletError>;
    async fn estimate_fee(&self, tx: &ChainTransaction) -> Result<Fee, WalletError>;
    async fn estimate_gas(&self, tx: TransactionRequest) -> Result<u64, WalletError>;
    fn validate_address(&self, address: &str) -> Result<(), WalletError>;
    fn chain_info(&self) -> ChainInfo;
    fn chain_type(&self) -> ChainType;
}
```

### 2. Core Layer (Layer 1) — Identical to Iced Design

`WalletState`, `Account`, `ChainTransaction`, `NetworkConfig`, `Storage` — all reused verbatim from the Iced design. See the Iced design document for full interface definitions.

### 3. Security Layer — Identical to Iced Design

`KeyringService`, `encrypt_data`/`decrypt_data`, `generate_mnemonic`/`derive_account` — all reused verbatim.

### 4. GUI Layer (Layer 2) — Dioxus

This is the primary difference from the Iced design. Dioxus replaces Iced with a component-based reactive model.

#### Application Root

```rust
// vaughan-dioxus/src/main.rs
fn main() {
    dioxus_desktop::launch_cfg(
        App,
        dioxus_desktop::Config::new()
            .with_window(WindowBuilder::new().with_title("Vaughan Wallet"))
    );
}

// vaughan-dioxus/src/gui/app.rs
#[component]
fn App() -> Element {
    // Provide WalletState globally
    use_context_provider(|| Signal::new(WalletState::new()));
    use_context_provider(|| Signal::new(AppView::Dashboard));

    rsx! {
        div { class: "wallet-app",
            Sidebar {}
            MainContent {}
        }
    }
}
```

#### State Management

Dioxus uses signals and context instead of Iced's message-passing:

```rust
// Global state via context
let wallet = use_context::<Signal<WalletState>>();
let current_view = use_context::<Signal<AppView>>();

// Async operations via coroutine
let signing_task = use_coroutine(|mut rx: UnboundedReceiver<SignRequest>| {
    to_owned![wallet];
    async move {
        while let Some(req) = rx.next().await {
            let result = wallet.read().sign_transaction(req.tx).await;
            // update UI state
        }
    }
});
```

#### View Components

```rust
pub enum AppView {
    Dashboard,
    Send,
    Receive,
    History,
    Settings,
    ImportExport,
}

// Each view is a Dioxus component
#[component]
fn Dashboard() -> Element { ... }

#[component]
fn SendView() -> Element { ... }

#[component]
fn ReceiveView() -> Element { ... }

#[component]
fn HistoryView() -> Element { ... }

#[component]
fn SettingsView() -> Element { ... }

#[component]
fn ImportExportView() -> Element { ... }
```

#### Theme System

Shared theme applied to both wallet and browser:

```rust
pub struct VaughanTheme {
    pub primary: &'static str,
    pub background: &'static str,
    pub surface: &'static str,
    pub text: &'static str,
    pub accent: &'static str,
    pub font_family: &'static str,
}

pub const THEME: VaughanTheme = VaughanTheme {
    primary: "#3366CC",
    background: "#F2F2F7",
    surface: "#FFFFFF",
    text: "#1A1A1A",
    accent: "#4D88FF",
    font_family: "Inter, system-ui, sans-serif",
};
```

#### Reusable Widgets

```rust
#[component]
fn AddressDisplay(address: String) -> Element {
    rsx! {
        div { class: "address-display",
            span { class: "address-text", "{&address[..6]}...{&address[address.len()-4..]}" }
            button { onclick: move |_| copy_to_clipboard(&address), "Copy" }
        }
    }
}

#[component]
fn BalanceDisplay(balance: Balance) -> Element { ... }

#[component]
fn TxStatusBadge(status: TxStatus) -> Element { ... }

#[component]
fn NetworkSelector() -> Element { ... }
```

#### Real-Time Updates

Balance watcher integrated via Dioxus coroutine:

```rust
let balance_updates = use_coroutine(|_: UnboundedReceiver<()>| {
    to_owned![wallet];
    async move {
        let mut watcher = BalanceWatcher::new(wallet.read().adapters.clone());
        loop {
            if let Ok(changes) = watcher.poll_balances().await {
                for change in changes {
                    wallet.write().update_balance(change);
                }
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }
});
```

### 5. dApp Browser — Tauri + Dioxus

The dApp browser is a separate Tauri application. Tauri provides the WebView host and OS-level process isolation. Dioxus provides the browser chrome UI (address bar, navigation, status). The wallet communicates with it via IPC.

#### Tauri Application Setup

```toml
# vaughan-tauri-browser/tauri.conf.json (excerpt)
{
  "package": { "productName": "Vaughan - dApp Browser", "version": "1.0.0" },
  "tauri": {
    "windows": [{
      "title": "Vaughan - dApp Browser",
      "width": 1200,
      "height": 800,
      "decorations": true
    }],
    "security": { "csp": "default-src 'self' https: data: blob:" }
  }
}
```

#### IPC Client in Tauri Backend

```rust
// vaughan-tauri-browser/src/main.rs
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let endpoint = &args[2]; // --ipc-endpoint
    let token = &args[4];    // --token

    // Connect to Dioxus wallet IPC server
    let mut stream = IpcStream::connect(endpoint).await.unwrap();
    stream.send(&Handshake { version: 1, token: token.clone() }).await.unwrap();

    // Share stream with Tauri state
    tauri::Builder::default()
        .manage(Arc::new(Mutex::new(stream)))
        .invoke_handler(tauri::generate_handler![
            handle_ethereum_request,
            get_accounts,
            sign_transaction,
        ])
        .run(tauri::generate_context!())
        .unwrap();
}

#[tauri::command]
async fn handle_ethereum_request(
    method: String,
    params: serde_json::Value,
    state: tauri::State<'_, Arc<Mutex<IpcStream>>>,
) -> Result<serde_json::Value, String> {
    let req = IpcRequest::from_rpc_method(&method, params)?;
    let mut stream = state.lock().await;
    stream.send(&req).await.map_err(|e| e.to_string())?;
    let resp: IpcResponse = stream.recv().await.map_err(|e| e.to_string())?;
    Ok(resp.into_json())
}
```

#### Dioxus Browser UI (inside Tauri)

```rust
// vaughan-tauri-browser/src-dioxus/browser_ui.rs
#[component]
fn BrowserApp() -> Element {
    let url = use_signal(|| "https://app.uniswap.org".to_string());
    let connected = use_signal(|| false);

    rsx! {
        div { class: "browser-chrome",
            div { class: "address-bar",
                button { onclick: move |_| { /* back */ }, "←" }
                button { onclick: move |_| { /* forward */ }, "→" }
                input {
                    value: "{url}",
                    oninput: move |e| url.set(e.value()),
                    onkeydown: move |e| if e.key() == Key::Enter { navigate(&url.read()) }
                }
                button { onclick: move |_| reload(), "↻" }
                ConnectionStatus { connected: *connected.read() }
            }
            // Tauri WebView renders the dApp here
        }
    }
}

#[component]
fn ConnectionStatus(connected: bool) -> Element {
    rsx! {
        span {
            class: if connected { "status connected" } else { "status disconnected" },
            if connected { "🔒 Connected" } else { "○ Not connected" }
        }
    }
}
```

#### EIP-1193 Provider Injection

The provider script is injected into every page loaded by the Tauri WebView. It communicates with the Tauri backend via `window.__TAURI__.invoke`:

```javascript
// provider.js — injected into WebView
class VaughanProvider {
    constructor() {
        this.isVaughan = true;
        this.isMetaMask = true; // legacy compat
        this._events = {};
    }

    async request({ method, params = [] }) {
        try {
            const result = await window.__TAURI__.invoke('handle_ethereum_request', {
                method,
                params
            });
            return result;
        } catch (error) {
            throw this._formatError(error);
        }
    }

    on(event, callback) {
        if (!this._events[event]) this._events[event] = [];
        this._events[event].push(callback);
    }

    removeListener(event, callback) {
        if (this._events[event]) {
            this._events[event] = this._events[event].filter(cb => cb !== callback);
        }
    }

    _emit(event, ...args) {
        (this._events[event] || []).forEach(cb => {
            try { cb(...args); } catch(e) { console.error(e); }
        });
    }

    _formatError(error) {
        const e = new Error(error.message || 'Unknown error');
        e.code = error.code || 4900;
        return e;
    }

    // Legacy compat
    enable() { return this.request({ method: 'eth_requestAccounts' }); }
    send(method, params) { return this.request({ method, params }); }
    sendAsync(payload, cb) {
        this.request({ method: payload.method, params: payload.params })
            .then(r => cb(null, { id: payload.id, jsonrpc: '2.0', result: r }))
            .catch(e => cb(e, null));
    }
}

const provider = new VaughanProvider();

// EIP-6963 announcement
const providerDetail = {
    info: {
        uuid: '350670db-19fa-4704-a166-e52e178b59d2',
        name: 'Vaughan Wallet',
        icon: 'data:image/svg+xml;base64,...',
        rdns: 'com.vaughan.wallet'
    },
    provider
};

window.addEventListener('eip6963:requestProvider', () => {
    window.dispatchEvent(new CustomEvent('eip6963:announceProvider', {
        detail: Object.freeze(providerDetail)
    }));
});
window.dispatchEvent(new CustomEvent('eip6963:announceProvider', {
    detail: Object.freeze(providerDetail)
}));

window.ethereum = provider;
window.dispatchEvent(new Event('ethereum#initialized'));

// Handle events from Tauri backend
window.__TAURI__.event.listen('wallet_event', ({ payload }) => {
    const { event, data } = payload;
    provider._emit(event, data);
    if (event === 'accountsChanged') provider._selectedAddress = data[0] || null;
    if (event === 'chainChanged') provider._chainId = data;
});
```

### 6. IPC Protocol — Shared Types

```rust
// vaughan-ipc-types/src/lib.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcRequest {
    GetAccounts,
    SignTransaction(SignTxPayload),
    SignMessage(SignMessagePayload),
    RequestApproval(ApprovalPayload),
    GetNetworkInfo,
    SwitchNetwork(NetworkId),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcResponse {
    Accounts(Vec<AccountInfo>),
    SignedTransaction(String),
    SignedMessage(String),
    ApprovalResult(bool),
    NetworkInfo(NetworkInfo),
    Error { code: u32, message: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Handshake {
    pub version: u32,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignTxPayload {
    pub from: String,
    pub to: String,
    pub value: String,
    pub data: Option<String>,
    pub chain_id: u64,
    pub gas_limit: Option<u64>,
    pub max_fee_per_gas: Option<u128>,
    pub max_priority_fee_per_gas: Option<u128>,
}
```

### 7. IPC Server in Dioxus Wallet

```rust
// In Dioxus wallet — spawned at startup
pub async fn start_ipc_server(
    wallet: Arc<Mutex<WalletState>>,
    approval_tx: mpsc::Sender<ApprovalRequest>,
) {
    let token = generate_secret_token();
    let listener = IpcListener::bind("vaughan-dapps").await.unwrap();

    loop {
        let stream = listener.accept().await.unwrap();
        let handshake: Handshake = stream.recv().await.unwrap();

        if handshake.token != token {
            tracing::warn!("IPC authentication failed");
            continue;
        }

        tokio::spawn(handle_browser_connection(stream, wallet.clone(), approval_tx.clone()));
    }
}

async fn handle_browser_connection(
    mut stream: IpcStream,
    wallet: Arc<Mutex<WalletState>>,
    approval_tx: mpsc::Sender<ApprovalRequest>,
) {
    loop {
        match stream.recv::<IpcRequest>().await {
            Ok(IpcRequest::GetAccounts) => {
                let accounts = wallet.lock().await.get_account_infos();
                stream.send(&IpcResponse::Accounts(accounts)).await.ok();
            }
            Ok(IpcRequest::SignTransaction(payload)) => {
                // Send to Dioxus UI for approval modal
                let (resp_tx, resp_rx) = oneshot::channel();
                approval_tx.send(ApprovalRequest { payload, resp_tx }).await.ok();
                let result = resp_rx.await.unwrap_or(Err(WalletError::UserRejected));
                match result {
                    Ok(hash) => stream.send(&IpcResponse::SignedTransaction(hash)).await.ok(),
                    Err(e) => stream.send(&IpcResponse::Error { code: 4001, message: e.to_string() }).await.ok(),
                };
            }
            Ok(IpcRequest::GetNetworkInfo) => {
                let info = wallet.lock().await.get_network_info();
                stream.send(&IpcResponse::NetworkInfo(info)).await.ok();
            }
            Err(_) => break,
            _ => {}
        }
    }
}
```

### 8. Approval Modals in Dioxus

```rust
#[component]
fn ApprovalModal(request: ApprovalRequest) -> Element {
    let tx = &request.payload;
    rsx! {
        div { class: "modal-overlay",
            div { class: "modal",
                h2 { "Approve Transaction" }
                div { class: "tx-details",
                    p { "To: " span { class: "mono", "{tx.to}" } }
                    p { "Value: " span { "{tx.value}" } }
                    p { "Network: " span { "Chain ID {tx.chain_id}" } }
                    if let Some(gas) = tx.max_fee_per_gas {
                        p { "Max Gas: " span { "{gas} wei" } }
                    }
                }
                div { class: "modal-actions",
                    button {
                        class: "btn-danger",
                        onclick: move |_| request.reject(),
                        "Reject"
                    }
                    button {
                        class: "btn-primary",
                        onclick: move |_| request.approve(),
                        "Approve"
                    }
                }
            }
        }
    }
}
```

## Launching the Browser

```rust
// In Dioxus wallet
pub fn launch_dapp_browser(token: &str) -> Result<Child, std::io::Error> {
    let browser_exe = find_browser_executable()?;

    Command::new(browser_exe)
        .arg("--ipc-endpoint").arg("vaughan-dapps")
        .arg("--token").arg(token)
        .spawn()
}

fn find_browser_executable() -> Result<PathBuf, std::io::Error> {
    // 1. Same directory as wallet executable
    let exe_dir = std::env::current_exe()?.parent().unwrap().to_path_buf();
    let candidate = exe_dir.join("vaughan-tauri-browser");
    if candidate.exists() { return Ok(candidate); }

    // 2. Platform standard locations
    #[cfg(windows)]
    let candidate = PathBuf::from(r"C:\Program Files\Vaughan\vaughan-tauri-browser.exe");
    #[cfg(target_os = "macos")]
    let candidate = PathBuf::from("/Applications/Vaughan.app/Contents/MacOS/vaughan-tauri-browser");
    #[cfg(target_os = "linux")]
    let candidate = PathBuf::from("/usr/bin/vaughan-tauri-browser");

    if candidate.exists() { return Ok(candidate); }

    // 3. PATH
    which::which("vaughan-tauri-browser").map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e))
}
```

## Window Integration and UX Cohesion

### Platform-Specific Window Grouping

**Windows — AppUserModelID**:
```rust
#[cfg(target_os = "windows")]
fn set_app_user_model_id() {
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    unsafe { SetCurrentProcessExplicitAppUserModelID("Vaughan.Wallet.1.0"); }
}
```

**macOS — Helper Process (no Dock icon)**:
```toml
# vaughan-tauri-browser/tauri.conf.json
{ "tauri": { "macOSPrivateApi": true } }
```
```xml
<!-- Info.plist -->
<key>LSUIElement</key><true/>
```

**Linux — WM_CLASS**:
```rust
#[cfg(target_os = "linux")]
fn set_wm_class(window: &Window) {
    // Set res_name and res_class to "vaughan-wallet" / "VaughanWallet"
}
```

### Browser Lifecycle

```rust
impl WalletApp {
    fn launch_browser(&mut self) {
        let token = generate_secret_token();
        if let Ok(child) = launch_dapp_browser(&token) {
            self.browser_handle = Some(child);
        }
    }

    fn on_exit(&mut self) {
        if let Some(mut child) = self.browser_handle.take() {
            let _ = child.kill();
        }
    }
}
```

## Data Models

All core data models (`AccountId`, `Balance`, `TxHash`, `TxRecord`, `TxStatus`, `Fee`, `TokenInfo`, `ChainType`, `WalletError`) are identical to the Iced design. See the Iced design document for full definitions.

## Correctness Properties

All 29 correctness properties from the Iced design apply unchanged, since the core logic is identical. The properties cover:

- Property 1: HD Wallet Derivation Correctness
- Property 2: Account Import Round-Trip
- Property 3: Transaction Signature Validity
- Property 4: Wallet Lock State Enforcement
- Property 5: Account Metadata Persistence Round-Trip
- Property 6: Keyring Storage Round-Trip
- Property 7: Encryption Round-Trip
- Property 8: Balance Query Validity
- Property 9: Transaction Broadcasting Returns Hash
- Property 10: Gas Estimation Positivity
- Property 11: Network Switching State Update
- Property 12: Transaction Validation Correctness
- Property 13: Nonce Sequentiality
- Property 14: EIP-1559 Gas Parameters Validity
- Property 15: Transaction Confirmation Tracking
- Property 16: Transaction History Caching Consistency
- Property 17: Token Transfer Transaction Creation
- Property 18: Transaction Status Validity
- Property 19: Keystore Persistence Round-Trip
- Property 20: Network Configuration Persistence Round-Trip
- Property 21: Token Configuration Persistence Round-Trip
- Property 22: Data Integrity Validation
- Property 23: Password Strength Validation
- Property 24: Authentication Rate Limiting
- Property 25: Multi-Account Type Support
- Property 26: Token Management Operations
- Property 27: Balance Polling Returns Updated Values
- Property 28: Balance Change Detection
- Property 29: Balance History Maintenance

Additional Dioxus/Tauri-specific properties:

### Property 30: IPC Message Serialization Round-Trip

*For any* IpcRequest or IpcResponse value, serializing to JSON and deserializing should return the same value.

**Validates: Requirements 19.3**

### Property 31: IPC Authentication Rejection

*For any* handshake with an incorrect token, the IPC server should reject the connection and not process any subsequent requests.

**Validates: Requirements 19.9**

### Property 32: Approval Modal Rejection Propagates

*For any* transaction signing request, if the user rejects the approval modal, the IPC response should contain error code 4001 (UserRejectedRequest).

**Validates: Requirements 19.11, 19.12**

## Testing Strategy

### Shared with Iced Design

All unit, property-based, integration, security, and performance testing strategies from the Iced design apply to the core modules unchanged.

### Dioxus-Specific Testing

**Component Tests**:
- Test each view component renders without panicking
- Test approval modal shows correct transaction details
- Test state updates propagate correctly via signals

**IPC Integration Tests**:
```rust
#[tokio::test]
async fn test_ipc_handshake_valid_token() {
    let token = "test_token_abc123";
    let listener = IpcListener::bind("test-wallet").await.unwrap();
    let mut client = IpcStream::connect("test-wallet").await.unwrap();
    let mut server = listener.accept().await.unwrap();

    client.send(&Handshake { version: 1, token: token.to_string() }).await.unwrap();
    let hs: Handshake = server.recv().await.unwrap();
    assert_eq!(hs.token, token);
}

#[tokio::test]
async fn test_get_accounts_flow() {
    let (mut server, mut client) = create_test_ipc_pair().await;
    client.send(&IpcRequest::GetAccounts).await.unwrap();
    let req: IpcRequest = server.recv().await.unwrap();
    assert!(matches!(req, IpcRequest::GetAccounts));
}
```

**EIP-1193 Compliance Tests**:
- Test all required JSON-RPC methods are handled
- Test all required events are emitted
- Test error codes match EIP-1193 standard
- Test EIP-6963 announcement fires on `eip6963:requestProvider`

**Local Test dApp**:
A local HTML page (`test-dapp/index.html`) is provided for manual and automated EIP-1193/6963 testing without relying on external dApps.

### Test Organization

```
tests/
├── unit/
│   ├── core/
│   ├── security/
│   └── chains/
├── property/
│   ├── core_properties.rs
│   ├── security_properties.rs
│   └── ipc_properties.rs
├── integration/
│   ├── ipc_integration.rs
│   ├── end_to_end.rs
│   └── eip1193_compliance.rs
└── benches/
    ├── signing_bench.rs
    └── ipc_latency_bench.rs
```

## Build, Packaging, and Release

### Cargo.toml (Workspace Root)

```toml
[workspace]
members = [
    "vaughan-core",
    "vaughan-dioxus",
    "vaughan-tauri-browser",
    "vaughan-ipc-types",
]

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

### Feature Flags

```toml
# vaughan-dioxus/Cargo.toml
[features]
default = []
audio = ["vaughan-core/audio"]
qr = ["qrcode", "image"]
hardware-wallets = ["vaughan-core/hardware-wallets"]
telemetry = ["sentry", "metrics"]
mobile = ["dioxus/mobile"]
```

### Binary Size Targets

| Configuration | Target Size |
|--------------|-------------|
| Minimal (no features) | < 15 MB |
| + audio | < 17 MB |
| + qr | < 16 MB |
| + hardware-wallets | < 18 MB |
| Full (all features) | < 25 MB |

### Platform Installers

| Platform | Format | Contents |
|----------|--------|----------|
| Windows | `.msi` | `vaughan-dioxus.exe` + `vaughan-tauri-browser.exe` |
| macOS | `.dmg` | `Vaughan.app` (both binaries in `Contents/MacOS/`) |
| Linux | `.deb` / `.AppImage` | Both binaries in `/usr/bin/` |
| Android | `.apk` | Dioxus wallet only (no browser on mobile) |
| iOS | `.ipa` | Dioxus wallet only (no browser on mobile) |

### Mobile Build Commands

```bash
# Android
cargo ndk -t arm64-v8a build --release -p vaughan-dioxus

# iOS
cargo build --release --target aarch64-apple-ios -p vaughan-dioxus
```

## Comparison: Iced vs Dioxus+Tauri Design

| Aspect | Iced Design | Dioxus+Tauri Design |
|--------|-------------|---------------------|
| GUI Framework | Iced (retained mode) | Dioxus (component/reactive) |
| State Management | Message passing (update fn) | Signals + context |
| Mobile Support | No | Yes (Android + iOS) |
| dApp Browser Host | Pure Dioxus process | Tauri (WebView host) |
| Browser IPC | interprocess | interprocess (same) |
| Provider Injection | Dioxus eval | Tauri invoke |
| Core Modules | Shared | Shared (identical) |
| Binary Count | 2 (iced + dioxus-browser) | 2 (dioxus + tauri-browser) |
| Desktop Targets | Windows, macOS, Linux | Windows, macOS, Linux |
| Mobile Targets | None | Android, iOS |
