use dioxus::prelude::*;

/// Vaughan-Tauri web (`Vaughan/web`) dark theme — CSS variables + layout utilities.
#[component]
pub fn ThemeStyles() -> Element {
    let css = r#"
:root {
  color-scheme: dark;

  /* Mirrors Tailwind `.dark` in Vaughan/web/src/index.css */
  --background: #000000;
  --foreground: #e2e8f0;
  --card: #0a0a0a;
  --card-foreground: #e2e8f0;
  --popover: #14141f;
  --popover-foreground: #e2e8f0;
  --primary: #e2e8f0;
  --primary-foreground: #0c0c0f;
  --secondary: #242428;
  --secondary-foreground: #e2e8f0;
  --muted: #242428;
  --muted-foreground: #8b95a8;
  --accent: #242428;
  --accent-foreground: #e2e8f0;
  --destructive: #9f2d2d;
  --destructive-foreground: #fafafa;
  --border: #2e2e34;
  --input: #1f1f24;
  --ring: #e2e8f0;

  --radius: 0px;
  --font-mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  --font-sans: system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial, "Apple Color Emoji", "Segoe UI Emoji";

  --success: #22c55e;
  --warning-bg: rgba(234, 179, 8, 0.1);
  --warning-border: rgba(234, 179, 8, 0.25);
  --warning-text: #eab308;
  --error-text: #f87171;
  --error-bg: rgba(239, 68, 68, 0.1);
}

html, body {
  background: var(--background);
  color: var(--foreground);
  margin: 0;
  padding: 0;
  font-family: var(--font-sans);
  font-size: 14px;
  line-height: 1.45;
}

*, *::before, *::after {
  box-sizing: border-box;
}

/* Shell: Layout.tsx — min-h-screen max-w-2xl */
.wallet-shell {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  padding: 16px;
  max-width: 42rem;
  margin: 0 auto;
  width: 100%;
}

.wallet-logo-block {
  margin-bottom: 16px;
}

/* Gradient wordmark when PNG logo is unavailable (index.css .vaughan-logo) */
.vaughan-logo-gradient {
  display: block;
  width: 100%;
  text-align: center;
  margin: 0;
  padding: 8px 0 4px;
  background: linear-gradient(90deg,
    #00ffff 0%, #00ff88 20%, #44ff44 40%, #ffaa00 60%, #ff6600 75%, #ff00ff 100%);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
  font-family: var(--font-mono);
  font-weight: 900;
  letter-spacing: 0.15em;
  font-size: clamp(1.5rem, 6vw, 2rem);
  text-transform: uppercase;
}

.wallet-tagline {
  margin: 6px 0 0 0;
  font-size: 12px;
  color: var(--muted-foreground);
}

.header-active-account {
  margin-top: 10px;
  text-align: center;
  padding: 0 4px;
  max-width: 100%;
}

.content-stack {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.actions-dock {
  padding-top: 16px;
}

/* vaughan-btn — square corners */
.vaughan-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  width: 100%;
  margin: 0;
  padding: 10px 14px;
  background: var(--card);
  border: 1px solid var(--border);
  color: var(--foreground);
  font-size: 13px;
  font-weight: 500;
  font-family: inherit;
  cursor: pointer;
  text-align: center;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.vaughan-btn:hover:not(:disabled) {
  background: var(--secondary);
}

.vaughan-btn:active:not(:disabled) {
  background: var(--accent);
}

.vaughan-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.actions-grid-4 {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
  width: 100%;
}

/* Legacy `.btn` — same as vaughan-btn */
.btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  margin: 0;
  padding: 10px 14px;
  background: var(--card);
  border: 1px solid var(--border);
  color: var(--foreground);
  font-size: 13px;
  font-weight: 500;
  font-family: inherit;
  cursor: pointer;
  text-align: center;
  transition: background 0.15s ease, border-color 0.15s ease;
}
.btn:hover:not(:disabled) {
  background: var(--secondary);
}
.btn:active:not(:disabled) {
  background: var(--accent);
}
.btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.btn-row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.btn-row .btn,
.btn-row .vaughan-btn {
  flex: 1;
}

.card-panel {
  background: var(--card);
  border: 1px solid var(--border);
  padding: 16px;
}

.card-panel.rounded-lg {
  border-radius: 8px;
}

.section-label {
  margin: 0 0 8px 0;
  font-size: 12px;
  font-weight: 600;
  color: var(--muted-foreground);
  display: flex;
  align-items: center;
  gap: 8px;
}

.field-label {
  display: block;
  margin: 0 0 6px 0;
  font-size: 12px;
  color: var(--muted-foreground);
}

.input-std {
  width: 100%;
  padding: 10px 12px;
  background: var(--input);
  border: 1px solid var(--border);
  color: var(--foreground);
  font-family: inherit;
  font-size: 13px;
  outline: none;
}

.input-std:focus {
  border-color: var(--ring);
  box-shadow: 0 0 0 1px var(--ring);
}

.input-mono {
  font-family: var(--font-mono);
  font-size: 12px;
}

.muted {
  color: var(--muted-foreground);
}

/* Subpages: Settings / History header */
.subpage-toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 24px;
}

.subpage-toolbar h1 {
  margin: 0;
  font-size: 1.25rem;
  font-weight: 600;
}

.icon-circle-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  padding: 0;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--muted-foreground);
  cursor: pointer;
  font-size: 18px;
  line-height: 1;
  transition: background 0.15s, color 0.15s;
}

.icon-circle-btn:hover {
  background: var(--secondary);
  color: var(--foreground);
}

/* DApps: centered title + absolute back */
.dapps-header-wrap {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 24px;
  min-height: 40px;
}

.dapps-back {
  position: absolute;
  left: 0;
  top: 50%;
  transform: translateY(-50%);
}

.dapps-title {
  margin: 0;
  font-size: 1.5rem;
  font-weight: 700;
}

/* Tauri DApps page: centered column */
.dapps-browser-shell {
  max-width: 64rem;
  margin: 0 auto;
  width: 100%;
  padding-top: 8px;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.dapps-selectors-row {
  display: flex;
  gap: 8px;
  width: 100%;
}

.dapps-selectors-row > div {
  flex: 1;
  min-width: 0;
}

/* Custom URL bar (Tauri parity) */
.dapp-url-bar-form {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 0;
  transition: border-color 0.15s, box-shadow 0.15s;
}

.dapp-url-bar-form:focus-within {
  border-color: rgba(226, 232, 240, 0.45);
  box-shadow: 0 0 0 1px rgba(226, 232, 240, 0.12);
}

.dapp-url-bar-input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: none;
  outline: none;
  color: var(--foreground);
  font-size: 13px;
  font-family: inherit;
  padding: 0 8px;
}

.dapp-url-bar-input::placeholder {
  color: var(--muted-foreground);
  opacity: 0.65;
}

.dapp-url-bar-plus {
  flex-shrink: 0;
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 8px;
  background: rgba(226, 232, 240, 0.08);
  color: var(--foreground);
  cursor: pointer;
  font-size: 20px;
  font-weight: 600;
  transition: background 0.15s;
}

.dapp-url-bar-plus:hover:not(:disabled) {
  background: rgba(226, 232, 240, 0.14);
}

.dapp-url-bar-go {
  flex-shrink: 0;
  padding: 10px 16px;
  border: none;
  border-radius: 8px;
  background: var(--primary);
  color: var(--primary-foreground);
  font-size: 13px;
  font-weight: 600;
  font-family: inherit;
  cursor: pointer;
  transition: opacity 0.15s;
}

.dapp-url-bar-go:hover:not(:disabled) {
  opacity: 0.92;
}

.dapp-url-bar-go:disabled,
.dapp-url-bar-plus:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* Primary CTA (onboarding, submit) */
.btn-primary-solid {
  width: 100%;
  padding: 12px 16px;
  border: none;
  border-radius: 8px;
  background: var(--primary);
  color: var(--primary-foreground);
  font-size: 14px;
  font-weight: 600;
  font-family: inherit;
  cursor: pointer;
  transition: opacity 0.15s;
}

.btn-primary-solid:hover:not(:disabled) {
  opacity: 0.92;
}

.btn-primary-solid:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-secondary-solid {
  width: 100%;
  padding: 12px 16px;
  border: none;
  border-radius: 8px;
  background: var(--secondary);
  color: var(--secondary-foreground);
  font-size: 14px;
  font-weight: 600;
  font-family: inherit;
  cursor: pointer;
  transition: background 0.15s;
}

.btn-secondary-solid:hover:not(:disabled) {
  filter: brightness(1.08);
}

/* Receive centered column */
.receive-stack {
  max-width: 28rem;
  margin: 0 auto;
  width: 100%;
  text-align: center;
  padding-top: 16px;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.back-link {
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0;
  border: none;
  background: none;
  color: var(--muted-foreground);
  font-size: 13px;
  cursor: pointer;
  font-family: inherit;
}

.back-link:hover {
  color: var(--foreground);
}

.receive-title {
  margin: 0;
  font-size: 1.5rem;
  font-weight: 700;
}

.receive-card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 32px 24px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 24px;
  box-shadow: 0 12px 40px rgba(0,0,0,0.35);
}

.qr-white-wrap {
  background: #fff;
  padding: 16px;
  border-radius: 8px;
}

.address-copy-row {
  width: 100%;
  text-align: left;
}

.address-copy-box {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 8px;
  padding: 12px;
  background: var(--input);
  border-radius: 8px;
  font-family: var(--font-mono);
  font-size: 11px;
  word-break: break-all;
  cursor: pointer;
  border: 1px solid transparent;
  transition: background 0.15s, border-color 0.15s;
}

.address-copy-box:hover {
  background: var(--accent);
  border-color: var(--border);
}

.warn-banner {
  font-size: 13px;
  color: var(--warning-text);
  background: var(--warning-bg);
  padding: 12px;
  border-radius: 8px;
  border: 1px solid var(--warning-border);
  text-align: left;
}

/* Onboarding home */
.onboarding-hero {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 16px;
}

.onboarding-inner {
  max-width: 28rem;
  width: 100%;
  text-align: center;
}

.onboarding-welcome {
  margin: 0 0 8px 0;
  font-size: 2rem;
  font-weight: 700;
  color: var(--primary);
}

.onboarding-card {
  margin-top: 32px;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 24px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  box-shadow: 0 8px 32px rgba(0,0,0,0.25);
}

/* Modal overlay (send confirm, hardware) */
.modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  background: rgba(0,0,0,0.78);
  backdrop-filter: blur(6px);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
}

.modal-sheet {
  width: 100%;
  max-width: 400px;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 20px 50px rgba(0,0,0,0.5);
}

/* History list */
.history-shell {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 8px;
  min-height: 400px;
}

.history-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 24px;
  flex-wrap: wrap;
  width: 100%;
}

.history-toolbar .subpage-toolbar {
  flex: 1;
  min-width: min(100%, 220px);
  margin-bottom: 0;
}

.tx-row {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  background: var(--background);
  border: 1px solid rgba(46, 46, 52, 0.6);
  border-radius: 6px;
  margin-bottom: 8px;
  transition: border-color 0.15s;
}

.tx-row:hover {
  border-color: var(--border);
}

.tx-icon-in {
  background: rgba(34, 197, 94, 0.12);
  color: var(--success);
}

.tx-icon-out {
  background: rgba(249, 115, 22, 0.12);
  color: #fb923c;
}

.tx-icon-wrap {
  width: 32px;
  height: 32px;
  border-radius: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  font-size: 14px;
}

/* dApp grid card (Tauri-style) */
.dapp-card {
  position: relative;
  overflow: hidden;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 0;
  padding: 16px;
  cursor: pointer;
  transition: border-color 0.15s, box-shadow 0.15s;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  min-height: 148px;
}

.dapp-card:hover {
  border-color: rgba(226, 232, 240, 0.45);
  box-shadow: 0 8px 24px rgba(0,0,0,0.25);
}

.dapp-card-icon-wrap {
  width: 40px;
  height: 40px;
  padding: 4px;
  border-radius: 8px;
  background: rgba(226, 232, 240, 0.05);
  border: 1px solid rgba(226, 232, 240, 0.12);
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  flex-shrink: 0;
}

.dapp-card-icon-wrap img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.dapp-card-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 8px;
}

.dapp-card-ext {
  font-size: 14px;
  color: var(--muted-foreground);
  flex-shrink: 0;
}

.dapp-card:hover .dapp-card-ext {
  color: var(--foreground);
}

.dapp-card-remove {
  position: absolute;
  top: 8px;
  right: 36px;
  padding: 6px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--muted-foreground);
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.15s, color 0.15s, background 0.15s;
  z-index: 2;
}

.dapp-card:hover .dapp-card-remove {
  opacity: 1;
}

.dapp-card-remove:hover {
  color: #ef4444;
  background: rgba(239, 68, 68, 0.12);
}

.dapp-card-cat {
  font-size: 10px;
  font-weight: 600;
  color: var(--muted-foreground);
  background: var(--secondary);
  padding: 2px 6px;
  border-radius: 2px;
  max-width: 72px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dapp-card-host {
  font-size: 10px;
  font-weight: 500;
  color: var(--muted-foreground);
  max-width: 96px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: right;
}

/* Square icon actions in the dApp card footer (rocket, PulseX play/stop, etc.) */
.dapp-card .dapp-card-icon-btn {
  min-width: 36px;
  min-height: 36px;
  padding: 0;
  flex-shrink: 0;
  box-sizing: border-box;
}
.dapp-card .dapp-card-icon-btn.dapp-card-icon-btn--rocket-on {
  opacity: 1;
  filter: grayscale(0) saturate(1.25);
}
.dapp-card .dapp-card-icon-btn.dapp-card-icon-btn--rocket-off {
  opacity: 0.35;
  filter: grayscale(1) saturate(0.2);
}

.dapp-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
}

@media (max-width: 520px) {
  .dapp-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

.danger-zone {
  border-color: rgba(239, 68, 68, 0.25);
}

.btn-danger-outline {
  width: 100%;
  padding: 10px 16px;
  background: var(--error-bg);
  border: 1px solid rgba(239, 68, 68, 0.25);
  color: var(--error-text);
  font-size: 13px;
  font-weight: 600;
  font-family: inherit;
  border-radius: 8px;
  cursor: pointer;
}

.btn-danger-outline:hover {
  background: rgba(239, 68, 68, 0.18);
}

.settings-section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--muted-foreground);
  margin: 0 0 12px 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.approval-card {
  margin-top: 14px;
  padding: 14px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--card);
}
"#;

    rsx! { style { "{css}" } }
}
