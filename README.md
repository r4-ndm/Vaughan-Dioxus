# Vaughan (Dioxus + Tauri)

Desktop wallet (**Dioxus**) and separate **Tauri** dApp browser, sharing **`vaughan-core`** and a local IPC bridge. Keys stay in the wallet process; the browser only forwards provider-style RPC.

> **Prototype status:** Vaughan is currently a prototype under active development. Expect rough edges, incomplete features, and breaking changes between revisions.

## Quick start

From the **repository root**, build the dApp browser and wallet, then launch the wallet (the browser binary must exist next to the wallet or under `target/debug` for trusted dApps):

```bash
cargo build -p vaughan-tauri-browser && cargo build -p vaughan-dioxus && cargo run -p vaughan-dioxus
```

For an optimized build, add `--release` to each of those three `cargo` invocations (see [DEVELOPER_GUIDE.md](DEVELOPER_GUIDE.md) for more detail).

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

Some websites and tools scan public repos and report “secrets.” Often they are wrong: test code uses fake passwords and dummy data on purpose. **Real checks:** run `cargo test --workspace`.

**Gitleaks (you can skip this):** [Gitleaks](https://github.com/gitleaks/gitleaks) is a separate program that searches the repo for things that *look* like leaked keys or passwords. You do **not** need it to build or run Vaughan. If you install it anyway, use [`.gitleaks.toml`](.gitleaks.toml) so it ignores a few files that only contain public addresses and test data. Command: `gitleaks detect --source . -c .gitleaks.toml`. Install: [releases page](https://github.com/gitleaks/gitleaks/releases), or `yay -S gitleaks` on Arch-based systems.

## License

Dual-licensed under **MIT OR Apache-2.0**, at your option, consistent with workspace `Cargo.toml`. The MIT text is in [LICENSE](LICENSE) and [LICENSE-MIT](LICENSE-MIT). For Apache-2.0, use the [standard license text](https://www.apache.org/licenses/LICENSE-2.0.txt).
