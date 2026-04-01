use dioxus::prelude::*;

/// Native balance row — similar to Vaughan dashboard asset strip.
#[component]
pub fn BalanceDisplay(amount: String, symbol: String) -> Element {
    rsx! {
        div { class: "card-panel",
            div { style: "display: grid; grid-template-columns: 1fr auto; align-items: center; gap: 12px; padding: 4px 0;",
                span { style: "font-weight: 700; font-size: 15px;", "{symbol}" }
                span { class: "muted", style: "font-size: 15px; text-align: right; font-variant-numeric: tabular-nums;",
                    "{amount}"
                }
            }
        }
    }
}
