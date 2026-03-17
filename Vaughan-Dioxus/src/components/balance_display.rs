use dioxus::prelude::*;

#[component]
pub fn BalanceDisplay(amount: String, symbol: String) -> Element {
    rsx! {
        div { style: "border: 1px solid var(--border); background: var(--card); padding: 14px;",
            p { class: "muted", style: "margin: 0; font-size: 12px;", "Balance" }
            div { style: "display: flex; align-items: baseline; gap: 10px; margin-top: 8px;",
                span { style: "font-size: 34px; font-weight: 700;", "{amount}" }
                span { class: "muted", style: "font-size: 14px;", "{symbol}" }
            }
        }
    }
}

