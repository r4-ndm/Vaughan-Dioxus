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

## Automated scanners

Some tools flag test-only strings (for example literals inside `#[cfg(test)]` modules) as “secrets.” Those fixtures are **not** production credentials. Run the real suite with `cargo test --workspace`.

## License

Dual-licensed under **MIT OR Apache-2.0**, at your option, consistent with workspace `Cargo.toml`. The MIT text is in [LICENSE](LICENSE) and [LICENSE-MIT](LICENSE-MIT). For Apache-2.0, use the [standard license text](https://www.apache.org/licenses/LICENSE-2.0.txt).
