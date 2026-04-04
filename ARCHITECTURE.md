# Vaughan Architecture Overview

## Reference implementation

**[Vaughan-Tauri](https://github.com/r4-ndm/Vaughan-Tauri)** (Tauri 2 + React shell, Alloy core) is the **working reference** for workflows, security posture, and Ethereum integration. This Dioxus + split-browser workspace is a rebuild that should stay **behaviorally and architecturally aligned** unless we record an explicit exception.

See **[`REFERENCE-Vaughan-Tauri.md`](REFERENCE-Vaughan-Tauri.md)** for principle parity, folder mapping (`src-tauri` ↔ `vaughan-core` / `Vaughan-Dioxus`), links to upstream specs, and the **local mirror** at **`Vaughan-Old/Vaughan-Tauri-main/Vaughan/`** (read/grep before GitHub when possible).

## High-Level Design

- **Core library (`vaughan-core`)**: Chain-agnostic wallet logic, security, monitoring, and chain adapters.
- **Dioxus desktop wallet (`vaughan-dioxus`)**: End-user wallet GUI that embeds `vaughan-core`.
- **Tauri dApp browser (`vaughan-tauri-browser`)**: Separate process that hosts web dApps and exposes a MetaMask-compatible provider.
- **IPC types (`vaughan-ipc-types`)**: Shared message types for wallet ↔ dApp browser communication.

The wallet and dApp browser are separate processes: the Dioxus wallet holds keys and signs; the Tauri browser only sees RPC-style provider calls over IPC.

## `vaughan-core`

- **Chains (`chains`)**
  - `ChainAdapter` trait: abstract interface for balance, nonce, fee estimation, send, history, and status.
  - `evm` adapter: Alloy-based EVM implementation, network configs, history fetching, and address utilities.
  - Shared types: `ChainType`, `EvmTransaction`, `Fee`, `TxRecord`, `TxStatus`, `TxHash`, `TokenInfo`, `Balance`.

- **Core services (`core`)**
  - `WalletState`: In-memory state for adapters, active chain, active account, lock flag, and high-level operations like `get_active_balance`, `estimate_fee`, `send_transaction`.
  - `AccountManager`: HD and imported account management backed by the keyring.
  - `TransactionService`: Building EVM and ERC-20 transactions, signing with Alloy, nonces, and broadcast via adapters.
  - `HistoryService`: Cached transaction and token transfer history per address/limit.
  - `NetworkService`: Built-in + custom network registry, active network selection, RPC health checks.
  - `StateManager`: File-based persistence for accounts, networks, and user preferences.
  - `TokenManager`: Per-chain registry of user-tracked ERC-20 tokens.

- **Security (`security`)**
  - `encryption`: Argon2id password hashing and AES-256-GCM password-based encryption/decryption.
  - `hd_wallet`: BIP-39/BIP-32/BIP-44 mnemonic + HD derivation helpers.
  - `KeyringService`: OS keychain integration, encrypting secrets before storage.
  - `AuthRateLimiter`: In-memory login/secret-access rate limiting with lockout.

- **Monitoring (`monitoring`)**
  - `BalanceWatcher`: Periodic polling with backoff to keep balances up to date.

- **Error handling (`error`)**
  - `WalletError`: Central error enum used across all layers.
  - `user_message()`: Convert internal errors into user-facing strings.
  - `retry_async()`: Exponential backoff wrapper for transient async failures.

- **Logging (`logging`)**
  - Tracing setup used by the Dioxus app and core.

## `vaughan-dioxus` (Wallet GUI)

- **Entry (`main.rs`)**
  - Initializes logging, constructs `WalletState` + core services, and launches the Dioxus app.

- **App shell (`app.rs`)**
  - Defines `AppServices` (shared `WalletState`, `NetworkService`, `HistoryService`, `AccountManager`, `TokenManager`).
  - Defines `AppRuntime` signals for balances and events.
  - Coordinates navigation between views (dashboard, send, receive, history, settings, import/export).

- **Theme and components**
  - `theme.rs`: CSS-variable-based dark theme and shared styling.
  - `components`: Reusable UI widgets (`AddressDisplay`, `BalanceDisplay`, `TxStatusBadge`, `NetworkSelector`).

- **Views**
  - `onboarding`: First-launch master wallet (password + recovery phrase) or restore; aligns with Vaughan-Tauri keychain + HD model.
  - `dashboard`: Overview of active account, balance, recent activity; uses `BalanceWatcher`.
  - `send`: Build/estimate/sign/broadcast transactions via `TransactionService` and `ChainAdapter`.
  - `receive`: Show active address, copy-to-clipboard, and optional QR code.
  - `history`: Load and display combined native + token history; poll pending tx status.
  - `settings`: Network management, preferences, and token tracking UI.
  - `import_export`: Master phrase export/replace, HD-derived accounts, imported keys (`AccountManager` + keyring); see [`REFERENCE-Vaughan-Tauri.md`](REFERENCE-Vaughan-Tauri.md) for parity with upstream wallet flows.

- **Utils**
  - `utils/clipboard`: Platform-aware clipboard abstraction used by views.

All Dioxus-side async flows ultimately delegate to `vaughan-core` services and chain adapters; the UI only handles user input, state wiring, and display.

## `vaughan-tauri-browser` (dApp Browser)

- Tauri app configured via `tauri.conf.json`.
- Hosts the dApp browser window and injects a MetaMask-compatible provider (EIP-1193 / EIP-6963 style).
- Uses `vaughan-ipc-types` to send provider-like requests to the Dioxus wallet process and receive responses (connect, sign, send, etc.).
- Never stores keys itself; only forwards RPC-style messages.

### Dual navigation modes (top-level URL)

The main webview uses one of two modes, chosen from `--url` using the **same host allowlist** as the wallet: `vaughan-trusted-hosts` (`ALLOWED_HTTPS_HOST_SUFFIXES` + `hostname_is_whitelisted`), used from `Vaughan-Dioxus/src/browser.rs` (`validate_whitelisted_dapp_url`) and `vaughan-tauri-browser/src/lib.rs`, with `provider_inject.js` and `index.html` kept in sync (enforced by `vaughan-trusted-hosts` unit tests). At runtime, **`navigate_trusted_dapp`** applies that same validation and calls `WebviewWindow::navigate` on `main`, so the shell or a trusted top-level page can switch to another allowlisted URL without respawning the process; capabilities mirror `remote.urls` plus local shell access, and Rust still rejects anything off-list.

| Mode | When | Main document | Chrome |
|------|------|---------------|--------|
| **External top-level** | `--url` is allowlisted `https://…` or loopback `http://` | `WebviewUrl::External` — dApp is the top document | Native **Navigation** menu: Back / Forward / Reload (shortcuts); no `index.html` iframe shell |
| **Shell** | No `--url`, empty `--url`, or non-allowlisted URL | `WebviewUrl::App("index.html")` — header, address bar, `#webview` iframe | In-page toolbar; Rust may set `__VAUGHAN_PENDING_INITIAL_URL` to open the iframe |

**Provider bridge:** `provider_inject.js` uses direct `__TAURI__.invoke` when available; nested cross-origin frames `postMessage` to `window.top` (shell `index.html` listener or, on external top-level, a `VAUGHAN_IPC` relay injected on allowlisted top documents). See `doc/TOPNAV-SPIKE.md` for the optional `spike_ping` / `VAUGHAN_SPIKE_EXTERNAL=1` probe.

**Wallet spawn:** The Dioxus wallet sets `VAUGHAN_WALLET_SPAWNED=1`, uses **piped stdin** for control, and usually starts a **warm** hidden shell at wallet startup (`VAUGHAN_WALLET_WARM_SHELL=1`, no `--url`). Opening a dApp sends `{"navigate_trusted":"<url>"}` on stdin (allowlist enforced in the browser); if that fails, the wallet respawns with `--url`. A monitor thread respawns after unexpected child exit using `last_url`. `VAUGHAN_NO_WARM_DAPP_BROWSER=1` disables warm spawn.

**Automated checks:** `cargo test -p vaughan-tauri-browser topnav_url_tests` asserts Uniswap / Aave / Sushi-style URLs resolve to external top-level and non-listed hosts to shell mode.

## IPC and Process Boundaries

- **Wallet process (Dioxus)**
  - Owns keys, runs `vaughan-core`, and exposes a small IPC server.
  - Handles account management, signing, and state transitions.

- **Browser process (Tauri)**
  - Runs web dApps and translates provider calls into IPC messages.
  - Receives signed transactions and status updates from the wallet.

This separation keeps private keys in the wallet process and treats the dApp browser as an untrusted client.

## Testing and Quality

- `vaughan-core` has unit tests for:
  - Security (encryption, HD wallet, keyring, password validation, rate limiting).
  - Persistence (roundtrip load/save).
  - Transactions (build + ERC-20 encoding).
  - History (cached retrieval and error propagation).
  - Network (custom networks and error paths).
  - Wallet state (lock/unlock, active account/chain, balance access).
  - Token manager (add/list/remove and validation).

These tests, combined with property tests for password validation and rate limiting, are used to drive Task 24.1’s coverage goal and to guard the critical wallet paths.

