use dioxus::prelude::*;

#[cfg(feature = "qr")]
use vaughan_core::chains::evm::utils::parse_address;

use crate::components::ColoredAddressText;
use crate::services::AppServices;
use crate::utils::clipboard::copy_text;

#[derive(Clone)]
pub struct ReceiveRuntime {
    pub last_status: Signal<Option<String>>,
}

#[component]
pub fn ReceiveView(on_back: Callback<()>) -> Element {
    let rt: ReceiveRuntime = use_context();
    let services: AppServices = use_context();

    let receive_address = use_signal(|| None::<String>);

    use_effect(move || {
        let mgr = services.account_manager.clone();
        let mut receive_address = receive_address.clone();
        spawn(async move {
            let s = match mgr.active_account().await {
                Some(a) => Some(format!("{:?}", a.address)),
                None => mgr
                    .list_accounts()
                    .await
                    .first()
                    .map(|a| format!("{:?}", a.address)),
            };
            receive_address.set(s);
        });
    });

    let on_row_copy = {
        let mut rt = rt.clone();
        let receive_address = receive_address.clone();
        move |_| {
            let addr_opt = receive_address.read().clone();
            let Some(addr) = addr_opt else {
                rt.last_status.set(Some(
                    "No wallet address yet. Finish onboarding or add an account.".into(),
                ));
                return;
            };
            match copy_text(&addr) {
                Ok(_) => rt.last_status.set(Some("Copied to clipboard".into())),
                Err(e) => rt.last_status.set(Some(format!("Clipboard error: {}", e))),
            }
        }
    };

    let address_for_display = receive_address
        .read()
        .clone()
        .unwrap_or_else(|| "—".to_string());

    let qr_section = {
        #[cfg(feature = "qr")]
        {
            let addr = receive_address.read().clone().unwrap_or_default();
            rsx! {
                div { class: "qr-white-wrap",
                    {render_qr(&addr)}
                }
            }
        }
        #[cfg(not(feature = "qr"))]
        {
            rsx! { span { style: "display: none;" } }
        }
    };

    rsx! {
        div { class: "receive-stack",
            button {
                class: "back-link",
                onclick: move |_| on_back.call(()),
                "← Back to Dashboard"
            }

            h1 { class: "receive-title", "Receive Assets" }

            if receive_address.read().is_some() {
                div { class: "receive-card",
                    {qr_section}

                    div { class: "address-copy-row",
                        p { class: "field-label", style: "text-align: center;", "Your address" }
                        div {
                            class: "address-copy-box",
                            onclick: on_row_copy,
                            ColoredAddressText { address: address_for_display.clone() }
                            span { class: "muted", style: "flex-shrink: 0; font-size: 12px;", "Copy" }
                        }
                    }

                    div { class: "warn-banner",
                        "Only send native tokens (ETH, PLS, etc.) and ERC-20 tokens to this address."
                    }

                    if let Some(msg) = rt.last_status.read().as_ref() {
                        p { class: "muted", style: "margin: 0; font-size: 12px;", "{msg}" }
                    }
                }
            } else {
                p { class: "muted", "Loading address…" }
            }
        }
    }
}

pub fn provide_receive_runtime() -> ReceiveRuntime {
    ReceiveRuntime {
        last_status: use_signal(|| None),
    }
}

#[cfg(feature = "qr")]
fn render_qr(text: &str) -> Element {
    use qrcode::render::svg;
    use qrcode::QrCode;

    let _ = parse_address(text);

    let code = QrCode::new(text.as_bytes()).ok();
    let svg_str = match code {
        Some(c) => c
            .render::<svg::Color>()
            .min_dimensions(200, 200)
            .quiet_zone(true)
            .build(),
        None => {
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"200\"></svg>".into()
        }
    };

    rsx! {
        div {
            dangerous_inner_html: "{svg_str}"
        }
    }
}
