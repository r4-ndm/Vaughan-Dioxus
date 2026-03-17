use dioxus::prelude::*;

#[component]
pub fn AddressDisplay(address: String) -> Element {
    let short = if address.len() > 12 {
        format!("{}...{}", &address[..6], &address[address.len() - 4..])
    } else {
        address.clone()
    };

    rsx! {
        div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
            p { class: "muted", style: "margin: 0; font-size: 12px;", "Address" }
            p { style: "margin: 8px 0 0 0; font-family: var(--font-mono); font-size: 12px; word-break: break-all;",
                "{address}"
            }
            p { class: "muted", style: "margin: 6px 0 0 0; font-size: 12px;", "{short}" }
        }
    }
}

