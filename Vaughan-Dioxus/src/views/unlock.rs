//! Password gate when a master wallet exists but the in-memory session password is empty (e.g. after restart).

use dioxus::prelude::*;
use futures_util::StreamExt;
use keyboard_types::Key;

use vaughan_core::error::WalletError;

use crate::services::AppServices;
use crate::theme::ThemeStyles;

const STARTUP_UNLOCK_KEY: &str = "startup_unlock";

#[derive(Debug, Clone)]
enum UnlockCmd {
    Try { password: String },
}

fn is_wrong_password_probe(e: &WalletError) -> bool {
    matches!(
        e,
        WalletError::DecryptionFailed(_) | WalletError::InvalidPassword
    )
}

#[component]
pub fn StartupUnlockView(on_unlocked: Callback<()>) -> Element {
    let services: AppServices = use_context();
    let mut password = use_signal(String::new);
    let error = use_signal(|| None::<String>);
    let busy = use_signal(|| false);

    let on_done = on_unlocked;
    let mut busy_c = busy;
    let mut error_c = error;
    let cmd_tx = use_coroutine(move |mut rx: UnboundedReceiver<UnlockCmd>| {
        let services = services.clone();
        let on_done = on_done;
        async move {
            while let Some(cmd) = rx.next().await {
                let UnlockCmd::Try { password: pw } = cmd;

                if services
                    .password_attempt_limiter
                    .is_locked(STARTUP_UNLOCK_KEY)
                    .await
                {
                    let mins = services
                        .password_attempt_limiter
                        .lockout_duration()
                        .as_secs()
                        / 60;
                    error_c.set(Some(format!(
                        "Too many failed attempts. Try again in about {mins} minutes."
                    )));
                    continue;
                }

                busy_c.set(true);
                error_c.set(None);

                // Must match onboarding/import: verify with the exact string stored in the keychain (no trim).
                let result = services.account_manager.verify_master_password(&pw);

                busy_c.set(false);

                match result {
                    Ok(()) => {
                        services
                            .password_attempt_limiter
                            .register_success(STARTUP_UNLOCK_KEY)
                            .await;
                        services.set_session_password(pw.clone()).await;
                        crate::chain_bootstrap::reconcile_and_sync_wallet_state(
                            services.wallet_state.as_ref(),
                            services.account_manager.as_ref(),
                            Some(pw.as_str()),
                        )
                        .await;
                        on_done.call(());
                    }
                    Err(e) => {
                        if is_wrong_password_probe(&e) {
                            match services
                                .password_attempt_limiter
                                .register_failure(STARTUP_UNLOCK_KEY)
                                .await
                            {
                                Ok(()) => error_c.set(Some(e.user_message())),
                                Err(_) => {
                                    let mins = services
                                        .password_attempt_limiter
                                        .lockout_duration()
                                        .as_secs()
                                        / 60;
                                    error_c.set(Some(format!(
                                        "Too many failed attempts. Try again in about {mins} minutes."
                                    )));
                                }
                            }
                        } else {
                            error_c.set(Some(e.user_message()));
                        }
                    }
                }
            }
        }
    });

    let try_unlock = move || {
        let pw = password.read().clone();
        if pw.is_empty() || *busy.read() {
            return;
        }
        cmd_tx.send(UnlockCmd::Try { password: pw });
    };

    rsx! {
        ThemeStyles {}
        div { class: "onboarding-hero",
            div { class: "onboarding-inner",
                h1 { class: "onboarding-welcome", "Unlock wallet" }
                p { class: "muted", style: "margin: 0; font-size: 14px;",
                    "Enter your wallet password to continue. Your session is cleared when you quit the app."
                }
                if let Some(err) = error.read().as_ref() {
                    div { style: "border: 1px solid rgba(239,68,68,0.3); background: var(--error-bg); padding: 12px; margin: 20px 0 0 0; border-radius: 8px; text-align: left;",
                        p { style: "margin: 0; color: var(--error-text); font-size: 13px;", "{err}" }
                    }
                }
                div { class: "onboarding-card",
                    input {
                        r#type: "password",
                        placeholder: "Wallet password",
                        value: "{password.read()}",
                        oninput: move |e| *password.write() = e.value(),
                        onkeydown: move |e: Event<KeyboardData>| {
                            if e.key() == Key::Enter {
                                try_unlock();
                            }
                        },
                        class: "input-std",
                    }
                    button {
                        class: "btn-primary-solid",
                        disabled: *busy.read() || password.read().is_empty(),
                        onclick: move |_| try_unlock(),
                        "Unlock"
                    }
                }
            }
        }
    }
}
