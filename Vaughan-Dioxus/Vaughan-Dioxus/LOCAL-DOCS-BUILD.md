# local-docs: Using Local Copies for the Build

This project has a **local-docs** folder (at `Vaughan-Dioxus/local-docs/`) with vendored/copied source for key dependencies. Building from these instead of the net is supported and recommended for offline work and version pinning.

## What’s in local-docs

| Folder | Content | Use for build? |
|--------|---------|----------------|
| **alloy** | Full Alloy workspace 1.7.3 (`crates/*`) | ✅ Yes. EVM provider, signers, RPC — everything we need. Use as path dep to workspace or the crates you need. |
| **dioxus** | Full Dioxus repo 0.7.3 (packages: dioxus, desktop, core, signals, router, web, etc.) | ✅ Yes. Wallet UI: depend on `dioxus` + `dioxus-desktop` via path. Examples and patterns in `examples/`. |
| **tauri** | Full Tauri 2 workspace (crates: tauri, tauri-runtime, tauri-build, etc. + examples) | ✅ Yes. dApp browser: depend on `tauri` (and related crates) via path. |
| **interprocess** | Single crate 2.4.0 (IPC: Unix domain sockets, named pipes) | ✅ Yes. Wallet ↔ dApp browser IPC. Use as path dep. |
| **rust-bip39** | bip39 2.2.2 (BIP-39 mnemonics) | ✅ Yes. HD wallet. Use as path dep. |
| **keyring-rs** | keyring 4.0.0-rc.3 (OS keychain) | ⚠️ Partial. This crate depends on **keyring-core** and platform crates from crates.io; those are not in local-docs. For a fully offline build you’d need to vendor those too, or use keyring 2.x from crates.io to match Vaughan-old. |
| **rust-bip32** | (if present) | BIP-32 derivation. Vaughan-old uses **coins-bip32** 0.8 from crates.io; local-docs has **iqlusioninc-crates/bip32** (different API). Prefer crates.io `coins-bip32` for compatibility with Vaughan-old unless we switch stack. |
| **iqlusioninc-crates** | bip32, secrecy, signatory, subtle-encoding, etc. | ✅ Reference / optional. We can use **secrecy** from here or crates.io. **bip32** here is a different crate than coins-bip32. |

## Verdict: Good for the build

- **Core stack is covered:** Alloy, Dioxus (including desktop), Tauri, interprocess, and rust-bip39 are full source trees in local-docs. We can build the wallet and dApp browser from these with path dependencies and no need to hit the net for them.
- **Caveats:**
  - **keyring**: Either use crates.io `keyring = "2.0"` (matches Vaughan-old) or vendor keyring-core (and platform stores) into local-docs for full offline.
  - **BIP32**: Use crates.io `coins-bip32 = "0.8"` to stay aligned with Vaughan-old; local-docs’ iqlusioninc bip32 is an alternative with a different API.
  - **Other deps** (serde, tokio, reqwest, aes-gcm, argon2, tracing, etc.) remain from crates.io unless we add them to local-docs.

## How to use in Cargo.toml

Example path deps (relative to workspace root; adjust if your crates live under `Vaughan-Dioxus/`):

```toml
# In vaughan-core or workspace Cargo.toml
[dependencies]
alloy = { path = "local-docs/alloy/crates/alloy" }           # or the specific alloy-* crates you need
# For Dioxus wallet (vaughan-dioxus):
dioxus = { path = "local-docs/dioxus/packages/dioxus" }
dioxus-desktop = { path = "local-docs/dioxus/packages/desktop" }
# For Tauri dApp browser (vaughan-tauri-browser):
tauri = { path = "local-docs/tauri/crates/tauri" }
# Shared:
interprocess = { path = "local-docs/interprocess" }
bip39 = { path = "local-docs/rust-bip39" }
# keyring / coins-bip32: use crates.io to match Vaughan-old, or add to local-docs later
keyring = "2.0"
coins-bip32 = "0.8"
```

Alloy is a workspace; you may need to point to the root and use workspace members, or depend on individual `alloy-*` crates under `alloy/crates/`.

## Summary

- **local-docs is in good shape for the build:** Alloy, Dioxus, Tauri, interprocess, and rust-bip39 are present and usable as the main offline/source dependencies.
- Use **crates.io** for keyring 2.0 and coins-bip32 to match Vaughan-old unless you later vendor or switch to the keyring-rs / iqlusion stacks in local-docs.
- Keep this file updated if you add or remove vendored crates.
