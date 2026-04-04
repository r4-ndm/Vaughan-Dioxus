//! First launch: create master wallet (password + BIP-39 seed) or restore from recovery phrase.

use dioxus::prelude::*;
use futures_util::StreamExt;

use vaughan_core::error::WalletError;
use vaughan_core::security::{generate_mnemonic, validate_password, PASSWORD_POLICY_DESCRIPTION};

use crate::services::AppServices;

#[derive(Debug, Clone)]
pub enum OnboardingCmd {
    FinishCreate { password: String, mnemonic: String },
    FinishRestore { password: String, mnemonic: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Pick,
    CreatePassword,
    CreateBackup,
    RestorePassword,
    RestoreMnemonic,
}

#[component]
pub fn OnboardingView(on_complete: Callback<()>) -> Element {
    let services: AppServices = use_context();

    let mut phase = use_signal(|| Phase::Pick);
    let mut password = use_signal(String::new);
    let mut password2 = use_signal(String::new);
    let mut generated_mnemonic = use_signal(String::new);
    let mut restore_mnemonic = use_signal(String::new);
    let mut backed_up = use_signal(|| false);
    let busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let on_done = on_complete;
    let mut busy_c = busy;
    let mut error_c = error;
    let cmd_tx = use_coroutine(move |mut rx: UnboundedReceiver<OnboardingCmd>| {
        let services = services.clone();
        let on_done = on_done;
        async move {
            while let Some(cmd) = rx.next().await {
                busy_c.set(true);
                error_c.set(None);

                let result: Result<(), WalletError> = async {
                    match cmd {
                        OnboardingCmd::FinishCreate { password, mnemonic } => {
                            services
                                .account_manager
                                .create_master_wallet(&password, &mnemonic)
                                .await?;
                            if !password.trim().is_empty() {
                                services.set_session_password(password).await;
                            }
                            Ok(())
                        }
                        OnboardingCmd::FinishRestore { password, mnemonic } => {
                            services
                                .account_manager
                                .replace_master_from_mnemonic(&password, &mnemonic)
                                .await?;
                            if !password.trim().is_empty() {
                                services.set_session_password(password).await;
                            }
                            Ok(())
                        }
                    }
                }
                .await;

                busy_c.set(false);
                match result {
                    Ok(()) => {
                        let pw = services.session_password().await;
                        crate::chain_bootstrap::reconcile_and_sync_wallet_state(
                            services.wallet_state.as_ref(),
                            services.account_manager.as_ref(),
                            pw.as_deref(),
                        )
                        .await;
                        on_done.call(());
                    }
                    Err(WalletError::InvalidPassword) => {
                        error_c.set(Some(PASSWORD_POLICY_DESCRIPTION.into()));
                    }
                    Err(e) => error_c.set(Some(e.user_message())),
                }
            }
        }
    });

    rsx! {
        div { class: "onboarding-hero",
            div { class: "onboarding-inner",
                h1 { class: "onboarding-welcome", "Welcome to Vaughan" }
                p { class: "muted", style: "margin: 0; font-size: 15px;",
                    "Secure, fast, private."
                }
                p { class: "muted", style: "margin: 12px 0 0 0; font-size: 12px; max-width: 28rem; margin-left: auto; margin-right: auto;",
                    "Set up your master wallet. Only this wallet has a recovery phrase; additional accounts are derived with your password."
                }

                if let Some(err) = error.read().as_ref() {
                    div { style: "border: 1px solid rgba(239,68,68,0.3); background: var(--error-bg); padding: 12px; margin: 20px 0 0 0; border-radius: 8px; text-align: left;",
                        p { style: "margin: 0; color: var(--error-text); font-size: 13px;", "{err}" }
                    }
                }

                match *phase.read() {
                    Phase::Pick => rsx! {
                        div { class: "onboarding-card",
                            p { class: "muted", style: "margin: 0; font-size: 13px; text-align: left;",
                                "Create a new master wallet or restore from your 12 or 24 word recovery phrase."
                            }
                            button {
                                class: "btn-primary-solid",
                                disabled: *busy.read(),
                                onclick: move |_| {
                                    error.set(None);
                                    password.set(String::new());
                                    password2.set(String::new());
                                    generated_mnemonic.set(String::new());
                                    backed_up.set(false);
                                    phase.set(Phase::CreatePassword);
                                },
                                "Create new wallet"
                            }
                            button {
                                class: "btn-secondary-solid",
                                disabled: *busy.read(),
                                onclick: move |_| {
                                    error.set(None);
                                    password.set(String::new());
                                    password2.set(String::new());
                                    restore_mnemonic.set(String::new());
                                    phase.set(Phase::RestorePassword);
                                },
                                "Restore with recovery phrase"
                            }
                        }
                    },
                    Phase::CreatePassword => rsx! {
                        div { class: "onboarding-card",
                            h2 { style: "margin: 0; font-size: 1.15rem; text-align: left;", "Wallet password" }
                            p { class: "muted", style: "margin: 0; font-size: 12px;",
                                "Encrypts your master seed in the OS keychain. "
                                {PASSWORD_POLICY_DESCRIPTION}
                            }
                            input {
                                r#type: "password",
                                placeholder: "Password",
                                value: "{password.read()}",
                                oninput: move |e| *password.write() = e.value(),
                                class: "input-std",
                            }
                            input {
                                r#type: "password",
                                placeholder: "Confirm password",
                                value: "{password2.read()}",
                                oninput: move |e| *password2.write() = e.value(),
                                class: "input-std",
                            }
                            div { class: "btn-row",
                                button {
                                    class: "vaughan-btn",
                                    disabled: *busy.read(),
                                    onclick: move |_| {
                                        error.set(None);
                                        phase.set(Phase::Pick);
                                    },
                                    "Back"
                                }
                                button {
                                    class: "btn-primary-solid",
                                    disabled: *busy.read(),
                                    onclick: move |_| {
                                        error.set(None);
                                        if password.read().as_str() != password2.read().as_str() {
                                            error.set(Some("Passwords do not match.".into()));
                                            return;
                                        }
                                        if validate_password(password.read().as_str()).is_err() {
                                            error.set(Some(PASSWORD_POLICY_DESCRIPTION.into()));
                                            return;
                                        }
                                        match generate_mnemonic(12) {
                                            Ok(p) => {
                                                generated_mnemonic.set(p);
                                                backed_up.set(false);
                                                phase.set(Phase::CreateBackup);
                                            }
                                            Err(e) => error.set(Some(e.user_message())),
                                        }
                                    },
                                    "Continue"
                                }
                            }
                        }
                    },
                    Phase::CreateBackup => rsx! {
                        div { class: "onboarding-card",
                            h2 { style: "margin: 0; font-size: 1.15rem; text-align: left;", "Back up your recovery phrase" }
                            p { class: "muted", style: "margin: 0; font-size: 12px;",
                                "Write these 12 words down and store them offline. They are the only way to recover your master wallet."
                            }
                            textarea {
                                readonly: true,
                                value: "{generated_mnemonic.read()}",
                                class: "input-std input-mono",
                                style: "min-height: 100px; resize: none;",
                            }
                            label { style: "display: flex; gap: 8px; align-items: flex-start; font-size: 13px;",
                                input {
                                    r#type: "checkbox",
                                    checked: *backed_up.read(),
                                    onchange: move |e| backed_up.set(e.value() == "true"),
                                }
                                span { "I have written down my recovery phrase in a safe place." }
                            }
                            div { class: "btn-row",
                                button {
                                    class: "vaughan-btn",
                                    disabled: *busy.read(),
                                    onclick: move |_| {
                                        error.set(None);
                                        phase.set(Phase::CreatePassword);
                                    },
                                    "Back"
                                }
                                button {
                                    class: "btn-primary-solid",
                                    disabled: *busy.read() || !*backed_up.read(),
                                    onclick: move |_| {
                                        error.set(None);
                                        let pw = password.read().clone();
                                        let m = generated_mnemonic.read().clone();
                                        cmd_tx.send(OnboardingCmd::FinishCreate {
                                            password: pw,
                                            mnemonic: m,
                                        });
                                    },
                                    "Open wallet"
                                }
                            }
                        }
                    },
                    Phase::RestorePassword => rsx! {
                        div { class: "onboarding-card",
                            h2 { style: "margin: 0; font-size: 1.15rem; text-align: left;", "Restore wallet" }
                            p { class: "muted", style: "margin: 0; font-size: 12px;",
                                "Choose a password to encrypt your wallet on this device. "
                                {PASSWORD_POLICY_DESCRIPTION}
                            }
                            input {
                                r#type: "password",
                                placeholder: "Password",
                                value: "{password.read()}",
                                oninput: move |e| *password.write() = e.value(),
                                class: "input-std",
                            }
                            input {
                                r#type: "password",
                                placeholder: "Confirm password",
                                value: "{password2.read()}",
                                oninput: move |e| *password2.write() = e.value(),
                                class: "input-std",
                            }
                            div { class: "btn-row",
                                button {
                                    class: "vaughan-btn",
                                    disabled: *busy.read(),
                                    onclick: move |_| {
                                        error.set(None);
                                        phase.set(Phase::Pick);
                                    },
                                    "Back"
                                }
                                button {
                                    class: "btn-primary-solid",
                                    disabled: *busy.read(),
                                    onclick: move |_| {
                                        error.set(None);
                                        if password.read().as_str() != password2.read().as_str() {
                                            error.set(Some("Passwords do not match.".into()));
                                            return;
                                        }
                                        if validate_password(password.read().as_str()).is_err() {
                                            error.set(Some(PASSWORD_POLICY_DESCRIPTION.into()));
                                            return;
                                        }
                                        phase.set(Phase::RestoreMnemonic);
                                    },
                                    "Continue"
                                }
                            }
                        }
                    },
                    Phase::RestoreMnemonic => rsx! {
                        div { class: "onboarding-card",
                            h2 { style: "margin: 0; font-size: 1.15rem; text-align: left;", "Recovery phrase" }
                            p { class: "muted", style: "margin: 0; font-size: 12px;",
                                "Paste your 12 or 24 word phrase. This replaces the current wallet on this device if one exists."
                            }
                            textarea {
                                placeholder: "word1 word2 …",
                                value: "{restore_mnemonic.read()}",
                                oninput: move |e| *restore_mnemonic.write() = e.value(),
                                class: "input-std input-mono",
                                style: "min-height: 100px; resize: none;",
                            }
                            div { class: "btn-row",
                                button {
                                    class: "vaughan-btn",
                                    disabled: *busy.read(),
                                    onclick: move |_| {
                                        error.set(None);
                                        phase.set(Phase::RestorePassword);
                                    },
                                    "Back"
                                }
                                button {
                                    class: "btn-primary-solid",
                                    disabled: *busy.read(),
                                    onclick: move |_| {
                                        error.set(None);
                                        let m = restore_mnemonic.read().trim().to_string();
                                        if m.is_empty() {
                                            error.set(Some("Enter your recovery phrase.".into()));
                                            return;
                                        }
                                        let pw = password.read().clone();
                                        cmd_tx.send(OnboardingCmd::FinishRestore {
                                            password: pw,
                                            mnemonic: m,
                                        });
                                    },
                                    "Restore wallet"
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
