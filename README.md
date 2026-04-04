# Vaughan (Dioxus + Tauri)

Desktop wallet (**Dioxus**) and separate **Tauri** dApp browser, sharing **`vaughan-core`** and a local IPC bridge. Keys stay in the wallet process; the browser only forwards provider-style RPC.

## Quick start

```bash
cargo build -p vaughan-tauri-browser
cargo run -p vaughan-dioxus
```

Release-style check:

```bash
cargo build --release -p vaughan-tauri-browser && cargo run --release -p vaughan-dioxus
```

## Layout

| Path | Role |
|------|------|
| `vaughan-core/` | Wallet logic, chains, security, persistence |
| `Vaughan-Dioxus/` | Wallet UI + IPC server + browser subprocess control |
| `vaughan-tauri-browser/` | Tauri webview + provider injection + IPC client |
| `vaughan-ipc-types/` | Shared IPC message types |
| `vaughan-trusted-hosts/` | Single Rust allowlist for trusted dApp HTTPS hosts (JS mirrors checked by tests) |
| `test-dapp/` | Minimal page for provider smoke tests |

Nested `Vaughan-Dioxus/Vaughan-Dioxus/` holds extra reference notes only; the **crate root** is `Vaughan-Dioxus/` (see workspace `Cargo.toml`).

## Docs

- [DEVELOPER_GUIDE.md](DEVELOPER_GUIDE.md) — build, env, dApp browser modes, warm browser
- [ARCHITECTURE.md](ARCHITECTURE.md) — processes and boundaries
- [USER_GUIDE.md](USER_GUIDE.md) — end-user oriented
- [SECURITY_CHECKLIST.md](SECURITY_CHECKLIST.md) — review checklist
- [Vaughan-Dioxus/tasks.md](Vaughan-Dioxus/tasks.md) — roadmap / task list

## CI

GitHub Actions runs `cargo fmt`, `clippy -D warnings`, tests, and `cargo audit` (see [.github/workflows/ci.yml](.github/workflows/ci.yml)).

**Lint config:** [`rustfmt.toml`](rustfmt.toml) and [`clippy.toml`](clippy.toml) at the repo root.

**Tests:** Unit tests live next to sources (`src/**/*.rs`); integration tests also live under `*/tests/*.rs` (for example [`vaughan-core/tests/crypto_smoke.rs`](vaughan-core/tests/crypto_smoke.rs)). Run `cargo test --workspace`.

**Docker (optional):** [`Dockerfile`](Dockerfile) mirrors the CI test command in a Debian bookworm image (installs Tauri Linux deps). Build with `docker build -t vaughan-ci .` — useful when you want a reproducible environment; it does not replace a normal desktop dev setup for the GUI.

## Automated scanners

Some tools flag test-only strings (for example literals inside `#[cfg(test)]` modules) as “secrets.” Those fixtures are **not** production credentials. Run the real suite with `cargo test --workspace`.

**Gitleaks (optional):** [`.gitleaks.toml`](.gitleaks.toml) extends the default rules and allowlists paths that intentionally contain public chain addresses, IPC test vectors, or curated dApp URLs. CI does not require it; install a binary if you want local checks, then run `gitleaks detect --source . -c .gitleaks.toml`. Examples: [GitHub releases](https://github.com/gitleaks/gitleaks/releases), `go install github.com/gitleaks/gitleaks/v8@latest`, or on Arch-based distros an AUR helper (e.g. `yay -S gitleaks`).

## License

Dual-licensed under **MIT OR Apache-2.0**, at your option, consistent with workspace `Cargo.toml`. The MIT text is in [LICENSE](LICENSE) and [LICENSE-MIT](LICENSE-MIT). For Apache-2.0, use the [standard license text](https://www.apache.org/licenses/LICENSE-2.0.txt).
