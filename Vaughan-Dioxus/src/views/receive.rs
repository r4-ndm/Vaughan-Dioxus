use dioxus::prelude::*;

#[cfg(feature = "qr")]
use vaughan_core::chains::evm::utils::parse_address;
use crate::utils::clipboard::copy_text;

#[derive(Clone)]
pub struct ReceiveRuntime {
    pub last_status: Signal<Option<String>>,
}

#[component]
pub fn ReceiveView() -> Element {
    let mut rt: ReceiveRuntime = use_context();

    // Until account selection is fully wired, use the same demo address used in Dashboard.
    let address = "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".to_string();
    let address_for_copy = address.clone();

    let on_copy = move |_| {
        match copy_text(&address_for_copy) {
            Ok(_) => rt.last_status.set(Some("Copied to clipboard".into())),
            Err(e) => rt.last_status.set(Some(format!("Clipboard error: {}", e))),
        };
    };

    let qr_section = {
        #[cfg(feature = "qr")]
        {
            rsx! {
                div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
                    p { class: "muted", style: "margin: 0; font-size: 12px;", "QR Code" }
                    {render_qr(&address)}
                    p { class: "muted", style: "margin-top: 10px; font-size: 12px;", "Scan to copy address." }
                }
            }
        }
        #[cfg(not(feature = "qr"))]
        {
            rsx! { span { style: "display: none;" } }
        }
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 12px;",
            h2 { "Receive" }

            div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
                p { class: "muted", style: "margin: 0; font-size: 12px;", "Your address" }
                p { style: "margin-top: 8px; margin-bottom: 0; font-family: var(--font-mono); font-size: 12px; word-break: break-all;",
                    "{address}"
                }
                div { class: "btn-row",
                    button { class: "btn", onclick: on_copy, "Copy" }
                }
                if let Some(msg) = rt.last_status.read().as_ref() {
                    p { class: "muted", style: "margin-top: 10px; font-size: 12px;", "{msg}" }
                }
            }

            {qr_section}
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

    // Validate to keep QR content sane; still render even if checksum-casing differs.
    let _ = parse_address(text);

    let code = QrCode::new(text.as_bytes()).ok();
    let svg_str = match code {
        Some(c) => c
            .render::<svg::Color>()
            .min_dimensions(180, 180)
            .quiet_zone(true)
            .build(),
        None => "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"180\" height=\"180\"></svg>".into(),
    };

    rsx! {
        div {
            style: "margin-top: 10px; background: #fff; padding: 10px; display: inline-block;",
            dangerous_inner_html: "{svg_str}"
        }
    }
}

