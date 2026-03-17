use dioxus::prelude::*;

/// Shared theme for wallet + browser (Task 14.5).
///
/// We keep this as CSS variables so it can later be mirrored inside the Tauri
/// browser UI (or any webview surface) with minimal drift.
#[component]
pub fn ThemeStyles() -> Element {
    let css = r#"
:root {
  --bg: #000000;
  --fg: #e6e6e6;
  --muted: rgba(230, 230, 230, 0.70);
  --border: #222222;
  --card: #0b0b0b;
  --card-2: #121212;
  --accent: #1a1a1a;

  --radius: 0px;
  --space-1: 6px;
  --space-2: 10px;
  --space-3: 16px;
  --space-4: 24px;

  --font-mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  --font-sans: system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial, "Apple Color Emoji", "Segoe UI Emoji";
}

html, body {
  background: var(--bg);
  color: var(--fg);
  margin: 0;
  padding: 0;
  font-family: var(--font-sans);
}

.wallet-shell {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  padding: var(--space-3);
  max-width: 720px;
  margin: 0 auto;
}

.topbar {
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border);
}

.logo {
  font-family: var(--font-mono);
  letter-spacing: 0.18em;
  font-weight: 900;
  margin: 0;
}

.muted {
  opacity: 0.8;
}

.content {
  flex: 1;
  padding-top: var(--space-3);
}

.btn-row {
  display: flex;
  gap: 8px;
  padding-top: 12px;
}

.btn {
  flex: 1;
  padding: 10px 12px;
  background: var(--card);
  border: 1px solid var(--border);
  color: var(--fg);
  font-size: 14px;
  cursor: pointer;
}

.btn:hover {
  background: var(--accent);
}
"#;

    rsx! { style { "{css}" } }
}

