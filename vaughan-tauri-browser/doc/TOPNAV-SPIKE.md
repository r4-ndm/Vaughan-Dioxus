# Top-level external URL spike (topnav-1)

## Question

When the main webview loads an **allowlisted** `https://` URL as the **top-level document** (`WebviewUrl::External`) instead of `index.html` + iframe, does JavaScript get `window.__TAURI__.core.invoke` so the wallet bridge can call `ipc_request` **without** a `postMessage` shell?

## Code analysis (Tauri 2.x)

In `tauri`’s `prepare_pending_webview` (`crates/tauri/src/manager/webview.rs`), **initialization scripts are always prepended** for every pending webview: invoke bootstrap, `__TAURI_INTERNALS__`, IPC template, plugins, and the app’s `initialization_script` / `initialization_script_for_all_frames`. There is **no branch** that skips this for `WebviewUrl::External`.

So **injection is not iframe-specific**: the same user-script bundle is intended to run on the **main frame** of `https://…` as well as `tauri://…` app pages.

Whether **`invoke` succeeds** for a given origin is a **second step**: Tauri 2 **capabilities** must list that origin under `remote.urls` and grant commands (e.g. `allow-ipc-request`). This repo already mirrors allowlisted dApp hosts in [`capabilities/default.json`](../capabilities/default.json).

## Empirical check (this repo)

**Product behavior (topnav-2):** an allowlisted `--url` (same host rules as wallet `browser.rs`) loads as **`WebviewUrl::External`** with **no** env var. **`VAUGHAN_SPIKE_EXTERNAL=1`** only adds the one-shot `spike_ping` init script plus **`.topnav_spike_*.txt`** probes below.

1. Build: `cargo build -p vaughan-tauri-browser` (or rely on `cargo run`, which builds if needed).
2. Run **with wallet IPC** (same as normal) **plus**:
   - `VAUGHAN_SPIKE_EXTERNAL=1` (for `spike_ping` + file probes only)
   - `--url https://app.uniswap.org` (or another allowlisted URL)

   Example:

   ```bash
   export VAUGHAN_SPIKE_EXTERNAL=1
   export RUST_LOG=vaughan_ipc_browser=debug
   cargo run -p vaughan-tauri-browser -- --ipc "$ENDPOINT" --token "$TOKEN" --url https://app.uniswap.org
   ```

   Prefer **`cargo run -p …`** so you always execute the binary Cargo just built (some environments only place the executable under `target/debug/deps/…` with a hash suffix, not as `target/debug/vaughan-tauri-browser`).

3. **`WebviewWindowBuilder::build()` blocks** until the native webview is created and the initial navigation has progressed. On a cold run against a heavy HTTPS dApp, **wait at least ~2 minutes** before concluding failure. Short `timeout 60s` runs often kill the process **before** `setup` finishes, so you will not see **`Vaughan dApp browser: main document is allowlisted external URL…`** or any `spike_ping` line.

4. Watch stderr: if **`TOPNAV_SPIKE: spike_ping invoked from JS`** appears, the **top-level** page successfully called `invoke('spike_ping')` — so **`__TAURI__` + capability ACL work** for that external origin.

5. File probes (no reliance on stderr flush): with `VAUGHAN_SPIKE_EXTERNAL=1`, the crate writes **`.topnav_spike_start.txt`** under `vaughan-tauri-browser/` (gitignored) when `run()` resolves the URL; after a successful invoke, **`.topnav_spike_invoke.txt`** contains `spike_ping_ok`.

If that line **never** appears but the page loads, open WebKit Web Inspector (debug build) and check the console for errors from the spike script or `invoke`.

## Preliminary conclusion (static analysis)

Tauri’s webview setup **always** attaches the invoke/IP C bootstrap to the pending webview, including `WebviewUrl::External`. **Capability `remote.urls`** is what allows a given `https` origin to **successfully call** `invoke`, not whether scripts are injected.

Empirical confirmation: run the spike below and look for **`TOPNAV_SPIKE: spike_ping invoked from JS`** on stderr (`RUST_LOG=vaughan_ipc_browser=debug` or default filter).

## Result (empirical, 2026-04-04)

| Check | Outcome |
|--------|---------|
| `spike_ping` log on stderr | **yes** — `TOPNAV_SPIKE: spike_ping invoked from JS` after `WebviewUrl::External` load |
| `.topnav_spike_invoke.txt` | **yes** — `spike_ping_ok` |
| URLs exercised | `https://app.uniswap.org` (allowlisted HTTPS); `http://127.0.0.1:…/` (loopback static page) |
| Platform | **Linux** (CachyOS), WebKit/Wry |
| Tauri version | **2.9.5** (from workspace `Cargo.lock`) |
| Notes | Confirms top-level external documents receive `window.__TAURI__.core.invoke` / `invoke` and can call `spike_ping` under `capabilities/default.json` `remote.urls` + `allow-spike-ping`. Allow enough runtime for `build()` + first navigation. |

## Notes / caveats

- GitHub issues exist for **edge cases** (e.g. some localhost or CEF paths) — if Linux WebKit behaves differently, record above.
- **CSP** on a specific site could theoretically interfere with injected scripts; Uniswap is used as a baseline because it already works in the iframe shell.
