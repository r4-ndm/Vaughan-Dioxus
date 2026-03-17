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

