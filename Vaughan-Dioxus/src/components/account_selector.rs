use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct AccountOption {
    pub name: String,
    pub address: String,
}

fn short_addr(a: &str) -> String {
    if a.len() < 14 {
        return a.to_string();
    }
    format!("{}...{}", &a[..8], &a[a.len() - 4..])
}

#[component]
pub fn AccountSelector(
    accounts: Vec<AccountOption>,
    active_address: Option<String>,
    on_select: Callback<String>,
) -> Element {
    let mut is_open = use_signal(|| false);
    let current_label = accounts
        .iter()
        .find(|a| active_address.as_deref() == Some(a.address.as_str()))
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "Select account".into());

    rsx! {
        div { style: "position: relative; width: 100%;",
            button {
                r#type: "button",
                class: "input-std",
                style: "width: 100%; display: flex; justify-content: space-between; align-items: center; cursor: pointer; text-align: left;",
                onclick: move |_| is_open.set(!is_open()),
                span { "{current_label}" }
                span { class: "muted", style: "font-size: 11px;", if is_open() { "▲" } else { "▼" } }
            }
            if is_open() {
                div {
                    style: "position: absolute; left: 0; right: 0; top: calc(100% + 4px); z-index: 60; border: 1px solid var(--border); background: var(--card);",
                    for acc in accounts {
                        button {
                            key: "{acc.address}",
                            r#type: "button",
                            style: "width: 100%; padding: 10px 12px; border-bottom: 1px solid var(--border); background: transparent; color: var(--fg); text-align: left; display: flex; justify-content: space-between; align-items: center; cursor: pointer;",
                            onclick: {
                                let address = acc.address.clone();
                                move |_| {
                                    on_select.call(address.clone());
                                    is_open.set(false);
                                }
                            },
                            span { "{acc.name}" }
                            if active_address.as_deref() == Some(acc.address.as_str()) {
                                span { style: "color: #22c55e; font-size: 12px;", "●" }
                            } else {
                                span { class: "muted", style: "font-family: var(--font-mono); font-size: 11px;", "{short_addr(&acc.address)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
