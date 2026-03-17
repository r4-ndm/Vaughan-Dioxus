# Using Vaughan-old as Architecture Reference

This document describes how **Vaughan-old** (`../Vaughan-old/Vaughan/`) is used as a guide for building Vaughan-Dioxus. The design and requirements docs already specify using "Vaughan-Iced and Vaughan-Tauri as reference blueprints"; Vaughan-old is the Tauri-based reference implementation.

## Architecture: Two Apps

- **Core wallet** — Built with **Dioxus** (dioxus-desktop on desktop, dioxus-mobile on Android/iOS). This is the main wallet app: dashboard, send, receive, history, settings, import/export. It uses the shared Rust crate **vaughan-core** (chains, core, security, monitoring) in-process. Keys and signing stay in this process.
- **dApp browser** — Built with **Tauri + Dioxus**: a separate executable/process. Tauri hosts the WebView (where arbitrary dApps run); Dioxus provides the browser chrome inside that window (address bar, back/forward, refresh, connection status). The browser talks to the wallet over IPC (interprocess); it never has access to keys.

So: **wallet = Dioxus (+ vaughan-core)**; **dApp browser = Tauri + Dioxus (separate process, IPC client)**.

## Why This Is a Good Idea

1. **Design doc alignment** — The Dioxus design says the wallet *core* (chains, core, security, monitoring) is **identical** to the Iced/Tauri design; only the GUI and dApp browser differ. Vaughan-old implements that core.

2. **Proven layering** — Vaughan-old already has the target architecture:
   - **Layer 0:** `chains/` — `ChainAdapter` trait, EVM adapter, types
   - **Layer 1:** `core/` — WalletService, TransactionService, NetworkService, persistence, price
   - **Security:** `security/` — keyring, encryption, HD wallet (BIP-39/BIP-32)
   - **Monitoring:** `monitoring/` — balance watcher with activity-based polling
   - **dApp:** `dapp/` — EIP-1193, approvals, proxy, session, window registry

3. **Same stack** — Alloy, keyring, bip39, coins-bip32, aes-gcm, argon2, secrecy, tracing. We keep the same crates in `vaughan-core`.

4. **Faster build** — We can port or adapt existing logic instead of re-deriving it, and use Vaughan-old’s tests as a checklist.

## What We Reuse vs Replace

| Area | Vaughan-old | Vaughan-Dioxus |
|------|-------------|----------------|
| **chains/** | ChainAdapter, EVM adapter, types | Port into `vaughan-core/chains/` — keep API, optionally refine |
| **core/** | wallet, transaction, network, persistence, price | Port into `vaughan-core/core/` — same logic, same traits |
| **security/** | encryption, hd_wallet, keyring_service | Port into `vaughan-core/security/` — no custom crypto |
| **monitoring/** | balance_watcher | Port into `vaughan-core/monitoring/` — same polling/events idea |
| **models/** | wallet, token, dapp, erc20 | Port into `vaughan-core/models/` (and ipc types where needed) |
| **error/** | WalletError variants | Port into `vaughan-core/error/` |
| **audio/** | SoundPlayer, config | Optional; port behind `audio` feature if desired |
| **Tauri commands** | commands/*.rs | **Replace** with Dioxus `use_coroutine` / backend calls from GUI |
| **state.rs** | VaughanState (Tauri-managed) | **Replace** with WalletState in Dioxus context (signals/context) |
| **dapp/** | In-process + proxy + windows | **Redesign**: dApp browser = separate Tauri process + IPC (see design.md) |
| **Frontend** | React (web/) | **Replace** with Dioxus GUI in `vaughan-dioxus/` |

## Key Files to Use as Guides

- **Architecture / layering:** `Vaughan-old/Vaughan/src-tauri/src/core/README.md`
- **Chain adapter API:** `Vaughan-old/Vaughan/src-tauri/src/chains/mod.rs`, `chains/evm/adapter.rs`
- **Wallet/account/transaction:** `core/wallet.rs`, `core/transaction.rs`, `core/network.rs`
- **Security:** `security/mod.rs`, `security/encryption.rs`, `security/hd_wallet.rs`, `security/keyring_service.rs`
- **State shape:** `state.rs` (VaughanState) → we reimplement as WalletState + Dioxus context
- **Balance watcher:** `monitoring/balance_watcher.rs`, `docs/architecture/BALANCE-POLLING.md`
- **dApp flow (concepts only):** `dapp/` for EIP-1193 and approvals; we redo as IPC + separate Tauri browser

## How to Use Vaughan-old During Development

1. **Scaffold `vaughan-core`** — Mirror `chains/`, `core/`, `security/`, `monitoring/`, `models/`, `error/` from Vaughan-old’s `src-tauri/src/`.
2. **Port, don’t copy-paste** — Adapt to workspace layout (e.g. `vaughan-core` crate), fix paths, and strip Tauri/React-specific code.
3. **Commands → Dioxus** — For each Tauri command that the UI needs, add a backend API (e.g. on WalletState or a service) and call it from Dioxus via `use_coroutine` or similar.
4. **dApp browser** — Implement from design.md (separate Tauri app, IPC with `vaughan-ipc-types`, EIP-1193 in WebView); use Vaughan-old’s dApp handling only as behavioral reference.
5. **Tests** — Use Vaughan-old’s unit/property tests as a checklist; reimplement or adapt in `vaughan-core` so they run in the new workspace.

## Location

- **Vaughan-old app:** `Vaughan-old/Vaughan/`
  - Rust backend: `Vaughan-old/Vaughan/src-tauri/`
  - React frontend: `Vaughan-old/Vaughan/web/`
  - Docs: `Vaughan-old/Vaughan/docs/`

Keep this reference doc updated as we add crates or change the mapping.
