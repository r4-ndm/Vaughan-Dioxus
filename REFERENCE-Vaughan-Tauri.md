# Reference: [Vaughan-Tauri](https://github.com/r4-ndm/Vaughan-Tauri)

This Dioxus workspace is a **rebuild** of the wallet and dApp stack. **[r4-ndm/Vaughan-Tauri](https://github.com/r4-ndm/Vaughan-Tauri)** is the **canonical reference** for workflows that are already implemented and exercised there: security assumptions, Alloy usage, EIP-1193 semantics, and multi-layer architecture.

## Local mirror (`Vaughan-Old`)

A **checked-in copy** of Vaughan-Tauri lives under this workspace (paths relative to repo root):

| Area | Path |
|------|------|
| Tauri app root (npm + `src-tauri` + `web`) | `Vaughan-Old/Vaughan-Tauri-main/Vaughan/` |
| Rust backend (chains, core, commands, security) | `Vaughan-Old/Vaughan-Tauri-main/Vaughan/src-tauri/src/` |
| React UI | `Vaughan-Old/Vaughan-Tauri-main/Vaughan/web/` |
| Public / provider assets | `Vaughan-Old/Vaughan-Tauri-main/Vaughan/public/` |
| Top-level repo README | `Vaughan-Old/Vaughan-Tauri-main/README.md` |

**Prefer reading and grepping these paths** when porting or comparing behavior; use GitHub only if something is missing locally or you need a specific revision.

Cursor agents load **`.cursor/rules/vaughan-tauri-reference.mdc`** when editing **`*.rs` / `*.js`** (not for README-only or other non-code work) so implementation stays aligned with Vaughan-Tauri while respecting the split-process security model.

When adding or changing behavior here, **prefer matching Vaughan-Tauri** unless we explicitly document an intentional deviation (e.g. UI framework, process split).

## Principles (from Vaughan-Tauri)

- **Alloy** — all Ethereum operations; no `ethers-rs` / ad-hoc crypto for chain work.
- **EIP-1193** — dApp-facing provider language (compatibility with MetaMask-style apps).
- **OS keychain** — private material not left in plain files; encryption helpers for secrets at rest.
- **Trait-based chains** — `ChainAdapter`-style boundaries for future non-EVM adapters.
- **No custom cryptography** — Argon2/AES/BIP crates + Alloy as in the reference stack.
- **Battle-tested patterns** — provider injection, CSP posture for embedded web, approval flows where applicable.

Upstream also points to deeper specs under `.kiro/specs/Vaughan-Tauri/` in that repo (requirements, design, security, testing). Use those when porting non-trivial features.

## Repository mapping

| Vaughan-Tauri | This workspace (Vaughan-Dioxus) |
|---------------|----------------------------------|
| `web/` (React UI) | `Vaughan-Dioxus/` (Dioxus `views/`, `app.rs`) |
| `src-tauri/src/chains/` | `vaughan-core/src/chains/` |
| `src-tauri/src/core/` | `vaughan-core/src/core/` (+ wallet-specific `wallet_ipc`, `dapp_approval` in Dioxus crate) |
| `src-tauri/src/commands/` | Wallet-side handlers: IPC in `wallet_ipc.rs`, approvals in `dapp_approval.rs`; browser uses Tauri `invoke` |
| `src-tauri/` single process (UI + Rust) | **Split:** Dioxus wallet process + `vaughan-tauri-browser` child process (`interprocess` / `vaughan-ipc-types`) |
| Tauri `initialization_script` provider | `vaughan-tauri-browser/provider_inject.js` + shell bridge in `index.html` |
| React “providers” | Dioxus context (`AppServices`, `WalletState`) |

## Intentional differences (document when extending)

1. **UI:** React → Dioxus (same responsibilities, different components).
2. **Process model:** Wallet and dApp browser are **separate binaries** for isolation; IPC replaces in-process Tauri commands for signing.
3. **Specs location:** This repo uses `design.md`, `requirements.md`, `ARCHITECTURE.md`; Vaughan-Tauri uses `.kiro/specs/…` — **cross-check both** when behavior should stay aligned.

## Contribution habit

Before merging a feature that exists in Vaughan-Tauri:

1. Locate the equivalent module or flow under **`Vaughan-Old/Vaughan-Tauri-main/Vaughan/`** (or on [GitHub](https://github.com/r4-ndm/Vaughan-Tauri) if not present locally).
2. Match user-visible behavior and security boundaries where possible.
3. Note any deliberate difference in the PR or in this file.
