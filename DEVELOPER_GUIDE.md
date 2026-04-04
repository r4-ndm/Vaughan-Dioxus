# Vaughan Developer Guide

## Prerequisites

- **Rust**: stable toolchain (via `rustup`), with `cargo` and `rustc` on `PATH`.
- **Node / npm**: only needed later if you add richer frontend tooling.
- **Windows**: current setup assumes x86_64-pc-windows-msvc (what you’re using).

## Project Layout

- `vaughan-core`: chain-agnostic wallet core (adapters, security, history, networks, persistence, tokens).
- `vaughan-dioxus`: Dioxus desktop wallet GUI.
- `vaughan-tauri-browser`: Tauri-based dApp browser (separate process).
- `vaughan-ipc-types`: shared IPC message types.

See `ARCHITECTURE.md` for detailed module and process diagrams.

## Building and Running

### Core library

From the workspace root:

```bash
cargo build -p vaughan-core
```

This compiles the wallet core as a regular Rust library crate.

### Dioxus desktop wallet

From the workspace root:

```bash
cargo run -p vaughan-dioxus
```

This launches the Dioxus desktop wallet using the default (desktop) platform.

### Tauri dApp browser

Currently the Tauri dApp browser is wired into the workspace but not fully configured for release:

- The crate is `vaughan-tauri-browser`.
- Windows builds require a valid `icons/icon.ico` binary; a placeholder task is in `tasks.md` (23.3).

To build (once a real icon is present):

```bash
cargo build -p vaughan-tauri-browser
```

#### IPC timing logs (wallet ↔ dApp browser)

The browser logs how long each wallet IPC round-trip takes (connection reuse vs new socket, request time, total). Logs use the Rust `tracing` crate with target `vaughan_ipc_browser`.

- **If you start the wallet from a terminal**, set the environment variable in that same shell **before** `cargo run`, so the spawned browser process inherits it:

  ```bash
  export RUST_LOG=vaughan_ipc_browser=debug
  cargo run -p vaughan-dioxus
  ```

  Then open a dApp from the wallet; timing lines appear in **that terminal** (along with other log output).

- **If `RUST_LOG` is unset**, the browser binary still enables `vaughan_ipc_browser=debug` by default in [`vaughan-tauri-browser/src/main.rs`](vaughan-tauri-browser/src/main.rs), so you still see IPC timing lines when stderr is visible (e.g. terminal launch).

**Request timeout:** each IPC call from the browser waits up to **10 seconds** for a response from the wallet. Fast calls (`GetNetworkInfo`, `eth_chainId`, etc.) are fine. If the user spends **longer than 10 seconds** on an approval dialog, the browser may report a timeout **even though** the wallet is still waiting (the wallet side allows longer for signing). Raising that limit is a small code change in `vaughan-tauri-browser` if you hit that in practice.

**Warm dApp browser:** After the wallet IPC server starts, the wallet spawns `vaughan-tauri-browser` once with a **hidden** shell (`index.html`) so process + WebKit are already up when you open the first trusted dApp; navigation uses a piped stdin line `{"navigate_trusted":"<url>"}` (no extra process). Set **`VAUGHAN_NO_WARM_DAPP_BROWSER=1`** on the wallet process to disable warm spawn (e.g. debugging).

**Why the dApp browser can still feel slow:** The **remote site** (Uniswap, Aave, etc.) dominates after warm-up. Use **`cargo build --release -p vaughan-tauri-browser`** for a faster binary. The browser **warms the wallet IPC pool in the background** as soon as the main window exists so the first provider RPC is less likely to wait on socket connect + handshake on top of page load.

**dApp browser modes (top-level URL):** When the wallet opens a **trusted** dApp, `--url` is allowlisted and the Tauri browser loads it as **`WebviewUrl::External`** (dApp is the main document, no `index.html` shell). Non-listed URLs still use the **shell** with the iframe and address bar. Details: [`ARCHITECTURE.md`](ARCHITECTURE.md) § `vaughan-tauri-browser`.

**In-app navigation:** From JS (shell or an allowlisted remote origin), `invoke('navigate_trusted_dapp', { url })` moves the **main** webview without restarting the browser process. The URL is checked again in Rust with the **same** allowlist as `--url`; use `--url` / wallet spawn when opening a **new** browser instance, and `navigate_trusted_dapp` when switching the existing window to another trusted dApp.

**TOPNAV spike (optional):** Set `VAUGHAN_SPIKE_EXTERNAL=1` to inject the one-shot `spike_ping` script and `.topnav_spike_*.txt` probes in addition to normal behavior. See [`vaughan-tauri-browser/doc/TOPNAV-SPIKE.md`](vaughan-tauri-browser/doc/TOPNAV-SPIKE.md).

Later, you can integrate `tauri dev` / `tauri build` flows as needed.

## Testing

### Core tests

Run the core test suite:

```bash
cargo test -p vaughan-core
```

This exercises:

- Security (encryption, HD wallet, keyring, password validation, rate limiting).
- Persistence (state file roundtrip).
- Transactions (EVM + ERC-20 building).
- History, network, wallet state, and token manager.

### Workspace tests

To run tests for all workspace members:

```bash
cargo test --workspace
```

On Windows, this currently depends on having a valid Tauri icon for `vaughan-tauri-browser`; see `tasks.md` 23.3.

### dApp browser (manual regression)

After `cargo build -p vaughan-tauri-browser` (and with the browser binary next to the wallet or under `target/debug`):

1. **Allowlisted top-level (e.g. Uniswap, Aave, Sushi)** — Open from the wallet DApps list. Expect: full-window dApp, **Navigation** menu (Back / Forward / Reload), wallet connects when you use the dApp.
2. **Shell mode** — If you can open the browser without a trusted URL (or paste a non-listed URL in the shell), expect: black header, iframe, address bar, IPC status dot; `about:blank` when no initial URL.
3. **Optional spike** — `VAUGHAN_SPIKE_EXTERNAL=1` with `cargo run -p vaughan-tauri-browser -- … --url https://app.uniswap.org`; allow time for webview creation; confirm stderr or `.topnav_spike_invoke.txt` per `TOPNAV-SPIKE.md`.

**Automated URL routing:** `cargo test -p vaughan-tauri-browser topnav_url_tests` checks allowlist → external vs shell resolution (no GUI).

## Mobile Targets (Deferred)

Mobile integration is intentionally deferred for now. When you’re ready to explore it:

- Add the appropriate Rust targets (`aarch64-linux-android`, `aarch64-apple-ios`, etc.).
- Use the Dioxus CLI (`dx serve --platform android`) as a starting point.
- Configure:
  - `AndroidManifest.xml` (Internet permission, etc.).
  - iOS entitlements and signing.

These steps are tracked as optional tasks in `tasks.md` (Task 17 and 26.2/26.3).

## Contributing Workflow (Local)

- Make changes in a dedicated branch (no pushes until explicitly approved).
- Run:
  - `cargo fmt`
  - `cargo clippy --all-targets --all-features` (when added)
  - `cargo test -p vaughan-core`
- Keep changes aligned with `tasks.md` and `ARCHITECTURE.md`.

As CI and release automation are implemented (Tasks 27.x), this guide can be extended with specific GitHub Actions and release steps.

