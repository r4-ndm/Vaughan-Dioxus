use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct NetworkOption {
    pub id: String,
    pub name: String,
    pub chain_id: u64,
}

#[component]
pub fn NetworkSelector(
    networks: Vec<NetworkOption>,
    active_id: Option<String>,
    on_select: Callback<String>,
) -> Element {
    rsx! {
        div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
            p { class: "muted", style: "margin: 0; font-size: 12px;", "Network" }

            select {
                style: "width: 100%; margin-top: 8px; padding: 10px 12px; background: var(--bg); border: 1px solid var(--border); color: var(--fg); font-size: 12px;",
                onchange: move |e| on_select.call(e.value()),
                for net in networks {
                    option {
                        value: "{net.id}",
                        selected: active_id.as_deref() == Some(net.id.as_str()),
                        "{net.name} (chain_id={net.chain_id})"
                    }
                }
            }
        }
    }
}
