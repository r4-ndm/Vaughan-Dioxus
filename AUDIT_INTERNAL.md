# Internal audit — Vaughan workspace

**Date:** 2026-03-29  
**Scope:** `vaughan-core`, `vaughan-ipc-types`, `Vaughan-Dioxus`, `vaughan-tauri-browser`, CI, dependency tree.  
**Not in scope:** External penetration test, formal cryptographic review, mobile-specific stores review.

## Executive summary

The codebase shows **deliberate security choices** (keychain-backed secrets, Argon2id/AES-GCM, IPC token handshake, request validation, rate limiting, structured logging without secrets in IPC paths). **Automated quality gates** are strong for `vaughan-core` and `vaughan-ipc-types` (`clippy -D warnings`). The **Dioxus shell** carries **~76 Clippy warnings** (mostly style: `clone_on_copy`, redundant closures) — acceptable short-term but worth tightening before a major release.

**Dependency tree** is large (Alloy/Tauri/Dioxus); duplicate crate versions are **mostly nested under Alloy** (e.g. `alloy-*` 1.5.x vs 1.7.x), which is **upstream layout**, not an obvious fix without Alloy upgrades.

**This environment could not run** `cargo audit` or a full **release** build (host `/tmp` / sandbox disk full). **Run locally or rely on GitHub Actions:** `cargo audit`, `cargo build --release`, optional `cargo bloat`.

---

## 1. Methodology

| Check | Result |
|--------|--------|
| `cargo tree -d` | Run; large output (~1700 lines), dominated by Alloy subgraph |
| `cargo clippy --workspace --all-targets` | **~76 warnings** (primarily `vaughan-dioxus`) |
| `cargo clippy -p vaughan-core -p vaughan-ipc-types -- -D warnings` | **Clean** (per CI policy) |
| `cargo test --workspace --lib` | **Green** (when using workspace `target/` + adequate disk) |
| `cargo audit` | **Not executed** here (tool not installed; disk limits on install) |
| Release binary size / `cargo bloat` | **Not executed** (C compiler temp files failed: no space) |
| Grep: `unsafe` in workspace `.rs` | **None found** |
| Grep: obvious secret logging | **No** `password` in tracing macros in quick scan |

---

## 2. Security (architecture & code)

### Strengths

- **Secrets:** Mnemonic/PK in OS keychain path; encryption helpers in `vaughan-core::security`.
- **Password policy:** `validate_password` + onboarding / import paths; **rate limiting** on import/export and startup unlock.
- **IPC:** Shared token on handshake; `IpcRequest::validate()` before handling; `tracing` for failures/approvals without logging payloads/secrets.
- **dApp surface:** Trusted host allowlist for opening URLs; browser in **separate process**; monitor respawn only when last URL retained after non-clean exit.
- **Session:** In-memory session password; unlock gate when master exists and session empty.

### Risks / follow-ups

| Item | Severity | Notes |
|------|----------|--------|
| IPC bound on filesystem socket / named pipe | Medium | Local attackers with same UID can attempt connection; **token** reduces risk; document threat model for users. |
| No TLS on IPC | Low (typical) | Local socket; acceptable if documented. |
| `eprintln!` on IPC server bind failures | Low | No secrets; prefer `tracing` for consistency. |
| Demo `WalletState` vs persisted accounts | Low–medium | **Improved:** `ensure_wallet_state_active_account` seeds `WalletState` from `AccountManager` when empty; balance/history use `primary_wallet_address_hex`. Residual drift if user switches account only in persistence without resyncing `WalletState`. |
| `PersistenceHandle::open().expect` / `AccountManager::new().expect` in `AppServices::new` | Medium | Startup **panic** if keyring/persistence fails — consider `Result` propagation or degraded mode for diagnostics. |

---

## 3. Dependency health & bloat

- **Duplicates:** `cargo tree -d` shows many entries; primary theme is **Alloy** pulling multiple internal versions. **Action:** Periodically bump workspace `alloy` and re-run `cargo tree -d`; no local fork needed unless duplicates cause bugs.
- **Audit:** Run **`cargo audit`** on every release branch (CI already includes a step when the runner has the tool).
- **Unused deps:** Not run (`cargo udeps` optional). Suggest one pass before 1.0.
- **Binary size:** Measure **`cargo build --release -p vaughan-dioxus`** and **`vaughan-tauri-browser`** on a clean machine; compare to `tasks.md` targets.

---

## 4. Code quality & duplication

### `unwrap` / `expect` / `panic`

- **Tests** and **proptest** use `unwrap` — acceptable.
- **Production-adjacent:** `RwLock` poison `expect` in persistence/account paths — standard pattern; ensure no long-held locks across `.await` (already documented in persistence).
- **GUI:** `services.rs` uses `expect` for `PersistenceHandle` and `AccountManager` init — **startup hard fail**; see above.
- **wallet_ipc:** Uses `unwrap_or(0)` on read lengths — fine; no panics on parse failure for handshake (early return).

### Duplicated logic

- **`dashboard.rs` and `history.rs`:** ~~Nearly identical EVM bootstrap~~ **Addressed (2026-03-29):** shared helpers in `Vaughan-Dioxus/src/chain_bootstrap.rs` (`evm_adapter_for_network_service`, `register_default_evm_adapter`, `primary_wallet_address_hex`, `ensure_wallet_state_active_account`).

### Clippy debt

- **Workspace ~76 warnings** with default lints, concentrated in **Dioxus** views (signals, clones).
- **Recommendation:** Fix in batches (unlock view, settings, send) or add **narrow** `allow` attributes only where Dioxus patterns fight the linter.

---

## 5. CI alignment

- **fmt + test + audit (when installed)** on Ubuntu.
- **Clippy:** strict on `vaughan-core` + `vaughan-ipc-types`; full workspace without `-D warnings`.
- **`TMPDIR: ${{ runner.temp }}`** — good for doctest/temp reliability.

---

## 6. Prioritized actions

1. **Run `cargo audit` + release build** on a machine with free disk; fix any critical advisories.
2. **Reduce demo/real account split** — single source of truth from `AccountManager` for dashboard/history/balance.
3. **Extract shared EVM adapter bootstrap** from dashboard/history.
4. **Soften `AppServices::new` panics** — return `Result` or `try_new` for friendlier failure modes.
5. **Chip away at Dioxus Clippy warnings** (or document policy).
6. **External audit** before high-stakes release (per `requirements.md`).

---

## 7. Task list cross-reference

- **`tasks.md` 22.5** — Full “security audit checklist” closure still requires **human sign-off** on manual items in `SECURITY_CHECKLIST.md` and optional **external** review.
- **`SECURITY_CHECKLIST.md`** — Update “Remaining” after each release prep (audit run, signing, manual recovery drill).

---

*This document is a living artifact; append dated reruns after major milestones.*
