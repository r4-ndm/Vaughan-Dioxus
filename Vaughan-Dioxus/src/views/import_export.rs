use dioxus::prelude::*;

use futures_util::StreamExt;

use crate::components::SubpageToolbar;
use crate::services::AppServices;
use vaughan_core::chains::evm::utils::parse_address;
use vaughan_core::core::account::AccountManager;
use vaughan_core::error::WalletError;
use vaughan_core::security::{validate_password, PASSWORD_POLICY_DESCRIPTION};

/// Rate-limit key for import/export password attempts (shared across all actions in this view).
const IMPORT_EXPORT_PW_KEY: &str = "import_export_password";
use crate::utils::clipboard::copy_text;

#[derive(Debug, Clone)]
pub enum ImportExportCmd {
    /// Replace master seed from recovery phrase (destructive). Prefer onboarding for first setup.
    ImportMnemonic {
        password: String,
        mnemonic: String,
    },
    ExportMnemonic {
        password: String,
    },
    /// Next HD index from stored master seed (wallet password only).
    AddHdAccount {
        password: String,
        name: String,
    },

    ImportPrivateKey {
        password: String,
        name: String,
        private_key: String,
    },
    ExportPrivateKey {
        password: String,
        address: String,
    },
}

#[derive(Clone)]
pub struct ImportExportRuntime {
    pub busy: Signal<bool>,
    pub error: Signal<Option<String>>,
    pub revealed_secret: Signal<Option<String>>,
}

pub fn provide_import_export_runtime() -> ImportExportRuntime {
    ImportExportRuntime {
        busy: use_signal(|| false),
        error: use_signal(|| None),
        revealed_secret: use_signal(|| None),
    }
}

#[component]
pub fn ImportExportView(cmd_tx: Coroutine<ImportExportCmd>, on_back: Callback<()>) -> Element {
    let mut rt: ImportExportRuntime = use_context();

    let mut password = use_signal(|| "".to_string());

    let mut mnemonic_in = use_signal(|| "".to_string());
    let mut hd_account_name = use_signal(|| "Account".to_string());
    let mut pk_name = use_signal(|| "Imported Account".to_string());
    let mut pk_in = use_signal(|| "".to_string());
    let mut export_addr = use_signal(|| "".to_string());

    let mut modal_open = use_signal(|| false);

    let mut open_modal = {
        let mut modal_open = modal_open.clone();
        move || *modal_open.write() = true
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 16px;",
            SubpageToolbar { title: "Accounts & keys", on_back: on_back.clone() }
            p { class: "muted", style: "font-size: 12px;",
                "The master wallet holds the only recovery phrase. Extra accounts are HD-derived with your wallet password. Exporting secrets shows them on screen — do that offline."
            }

            div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
                p { class: "muted", style: "margin: 0; font-size: 12px;", "Password (required for keychain encryption/decryption)" }
                p { class: "muted", style: "margin: 8px 0 0 0; font-size: 11px;", {PASSWORD_POLICY_DESCRIPTION} }
                input {
                    r#type: "password",
                    value: "{password.read()}",
                    oninput: move |e| *password.write() = e.value(),
                    style: "width: 100%; margin-top: 8px; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-family: var(--font-mono); font-size: 12px;",
                    placeholder: "Your wallet password"
                }
            }

            if let Some(err) = rt.error.read().as_ref() {
                div { style: "border: 1px solid #442; background: #110; padding: 12px;",
                    p { style: "margin: 0; color: #f5b;", "{err}" }
                }
            }

            // ----- Master recovery phrase -----
            div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
                h3 { style: "margin: 0 0 8px 0;", "Master recovery phrase" }
                p { class: "muted", style: "margin: 0 0 10px 0; font-size: 12px;",
                    "Only the master wallet has a seed phrase. Export shows it after you enter your password. Importing a phrase replaces the current master wallet and clears account metadata on this device."
                }
                div { class: "btn-row",
                    button {
                        class: "btn",
                        disabled: *rt.busy.read(),
                        onclick: move |_| {
                            rt.error.set(None);
                            rt.revealed_secret.set(None);
                            cmd_tx.send(ImportExportCmd::ExportMnemonic { password: password.read().clone() });
                            open_modal();
                        },
                        "Export master phrase"
                    }
                }

                p { class: "muted", style: "margin: 10px 0 6px 0; font-size: 12px;", "Replace master from phrase (destructive)" }
                textarea {
                    value: "{mnemonic_in.read()}",
                    oninput: move |e| *mnemonic_in.write() = e.value(),
                    style: "width: 100%; min-height: 90px; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-family: var(--font-mono); font-size: 12px;",
                    placeholder: "twelve or twenty-four words …"
                }
                div { class: "btn-row",
                    button {
                        class: "btn",
                        disabled: *rt.busy.read(),
                        onclick: move |_| {
                            rt.error.set(None);
                            cmd_tx.send(ImportExportCmd::ImportMnemonic {
                                password: password.read().clone(),
                                mnemonic: mnemonic_in.read().trim().to_string(),
                            });
                        },
                        "Replace master from phrase"
                    }
                }
            }

            // ----- HD derived account (no new seed) -----
            div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
                h3 { style: "margin: 0 0 8px 0;", "Add HD account" }
                p { class: "muted", style: "margin: 0 0 10px 0; font-size: 12px;",
                    "Derives the next address from your stored master seed. Requires your wallet password above — no recovery phrase."
                }
                input {
                    value: "{hd_account_name.read()}",
                    oninput: move |e| *hd_account_name.write() = e.value(),
                    style: "width: 100%; margin-top: 8px; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-size: 12px;",
                    placeholder: "Account name"
                }
                div { class: "btn-row",
                    button {
                        class: "btn",
                        disabled: *rt.busy.read(),
                        onclick: move |_| {
                            rt.error.set(None);
                            cmd_tx.send(ImportExportCmd::AddHdAccount {
                                password: password.read().clone(),
                                name: hd_account_name.read().trim().to_string(),
                            });
                        },
                        "Add HD account"
                    }
                }
            }

            // ----- Private key -----
            div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
                h3 { style: "margin: 0 0 8px 0;", "Private key" }
                p { class: "muted", style: "margin: 0 0 10px 0; font-size: 12px;",
                    "Import stores the key encrypted in OS keychain. Export requires password."
                }

                p { class: "muted", style: "margin: 0; font-size: 12px;", "Account name" }
                input {
                    value: "{pk_name.read()}",
                    oninput: move |e| *pk_name.write() = e.value(),
                    style: "width: 100%; margin-top: 8px; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-size: 12px;",
                }

                p { class: "muted", style: "margin: 10px 0 0 0; font-size: 12px;", "Private key (hex or 0x-hex)" }
                input {
                    value: "{pk_in.read()}",
                    oninput: move |e| *pk_in.write() = e.value(),
                    style: "width: 100%; margin-top: 8px; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-family: var(--font-mono); font-size: 12px;",
                    placeholder: "0x…"
                }
                div { class: "btn-row",
                    button {
                        class: "btn",
                        disabled: *rt.busy.read(),
                        onclick: move |_| {
                            rt.error.set(None);
                            cmd_tx.send(ImportExportCmd::ImportPrivateKey {
                                password: password.read().clone(),
                                name: pk_name.read().clone(),
                                private_key: pk_in.read().trim().to_string(),
                            });
                        },
                        "Import private key"
                    }
                }

                p { class: "muted", style: "margin: 10px 0 0 0; font-size: 12px;", "Export private key for address" }
                input {
                    value: "{export_addr.read()}",
                    oninput: move |e| *export_addr.write() = e.value(),
                    style: "width: 100%; margin-top: 8px; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-family: var(--font-mono); font-size: 12px;",
                    placeholder: "0x…"
                }
                div { class: "btn-row",
                    button {
                        class: "btn",
                        disabled: *rt.busy.read(),
                        onclick: move |_| {
                            rt.error.set(None);
                            rt.revealed_secret.set(None);
                            cmd_tx.send(ImportExportCmd::ExportPrivateKey {
                                password: password.read().clone(),
                                address: export_addr.read().trim().to_string(),
                            });
                            open_modal();
                        },
                        "Export private key"
                    }
                }
            }

            if *modal_open.read() {
                div {
                    style: "position: fixed; inset: 0; background: rgba(0,0,0,0.7); display: flex; align-items: center; justify-content: center; padding: 16px;",
                    onclick: move |_| *modal_open.write() = false,
                    div {
                        style: "width: 100%; max-width: 720px; background: var(--card-2); border: 1px solid var(--border); padding: 16px;",
                        onclick: move |evt| evt.stop_propagation(),
                        h3 { "Sensitive data" }
                        p { class: "muted", style: "font-size: 12px;", "Do not share. Close this window when done." }

                        if let Some(secret) = rt.revealed_secret.read().as_ref() {
                            textarea {
                                value: "{secret}",
                                readonly: true,
                                style: "width: 100%; min-height: 120px; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-family: var(--font-mono); font-size: 12px;",
                            }
                            div { class: "btn-row",
                                button {
                                    class: "btn",
                                    onclick: {
                                        let s = secret.clone();
                                        move |_| {
                                            let _ = copy_text(&s);
                                        }
                                    },
                                    "Copy"
                                }
                                button { class: "btn", onclick: move |_| *modal_open.write() = false, "Close" }
                            }
                        } else {
                            p { class: "muted", "Loading / nothing to show." }
                            div { class: "btn-row",
                                button { class: "btn", onclick: move |_| *modal_open.write() = false, "Close" }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn use_import_export_coroutine() -> Coroutine<ImportExportCmd> {
    let services: AppServices = use_context();
    let rt: ImportExportRuntime = use_context();

    use_coroutine(move |mut rx: UnboundedReceiver<ImportExportCmd>| {
        let services = services.clone();
        let mut rt2 = rt.clone();
        async move {
            let mgr: &AccountManager = services.account_manager.as_ref();

            while let Some(cmd) = rx.next().await {
                rt2.busy.set(true);
                rt2.error.set(None);

                let password_for_policy_check: Option<&str> = match &cmd {
                    ImportExportCmd::ImportMnemonic { password, .. }
                    | ImportExportCmd::ImportPrivateKey { password, .. } => Some(password.as_str()),
                    _ => None,
                };
                if let Some(pw) = password_for_policy_check {
                    if validate_password(pw.trim()).is_err() {
                        rt2.error.set(Some(PASSWORD_POLICY_DESCRIPTION.to_string()));
                        rt2.busy.set(false);
                        continue;
                    }
                }

                if services
                    .password_attempt_limiter
                    .is_locked(IMPORT_EXPORT_PW_KEY)
                    .await
                {
                    let mins = services
                        .password_attempt_limiter
                        .lockout_duration()
                        .as_secs()
                        / 60;
                    rt2.error.set(Some(format!(
                        "Too many failed password attempts. Try again in about {mins} minutes."
                    )));
                    rt2.busy.set(false);
                    continue;
                }

                let result: Result<(), WalletError> = (|| async {
                    match cmd {
                        ImportExportCmd::ImportMnemonic { password, mnemonic } => {
                            if !password.trim().is_empty() {
                                services.set_session_password(password.clone()).await;
                            }
                            mgr.replace_master_from_mnemonic(&password, &mnemonic)
                                .await?;
                            Ok(())
                        }
                        ImportExportCmd::AddHdAccount { password, name } => {
                            if !password.trim().is_empty() {
                                services.set_session_password(password.clone()).await;
                            }
                            let n = if name.trim().is_empty() {
                                "HD account".into()
                            } else {
                                name
                            };
                            mgr.add_hd_derived_account(&password, n).await?;
                            Ok(())
                        }
                        ImportExportCmd::ExportMnemonic { password } => {
                            if !password.trim().is_empty() {
                                services.set_session_password(password.clone()).await;
                            }
                            let phrase = mgr.export_wallet_mnemonic(&password)?;
                            rt2.revealed_secret.set(Some(phrase));
                            Ok(())
                        }
                        ImportExportCmd::ImportPrivateKey {
                            password,
                            name,
                            private_key,
                        } => {
                            if !password.trim().is_empty() {
                                services.set_session_password(password.clone()).await;
                            }
                            let _acct = mgr
                                .import_private_key_account(&password, &private_key, name)
                                .await?;
                            Ok(())
                        }
                        ImportExportCmd::ExportPrivateKey { password, address } => {
                            if !password.trim().is_empty() {
                                services.set_session_password(password.clone()).await;
                            }
                            let addr = parse_address(&address)?;
                            let pk = mgr.export_private_key(&password, addr)?;
                            rt2.revealed_secret.set(Some(format!("0x{pk}")));
                            Ok(())
                        }
                    }
                })()
                .await;

                if let Err(e) = result {
                    if matches!(e, WalletError::InvalidPassword) {
                        match services
                            .password_attempt_limiter
                            .register_failure(IMPORT_EXPORT_PW_KEY)
                            .await
                        {
                            Ok(()) => rt2.error.set(Some(e.user_message())),
                            Err(_) => {
                                let mins = services
                                    .password_attempt_limiter
                                    .lockout_duration()
                                    .as_secs()
                                    / 60;
                                rt2.error.set(Some(format!(
                                    "Too many failed password attempts. Try again in about {mins} minutes."
                                )));
                            }
                        }
                    } else {
                        rt2.error.set(Some(e.user_message()));
                    }
                } else {
                    services
                        .password_attempt_limiter
                        .register_success(IMPORT_EXPORT_PW_KEY)
                        .await;
                }
                rt2.busy.set(false);
            }
        }
    })
}
