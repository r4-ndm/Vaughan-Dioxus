use dioxus::prelude::*;

use vaughan_core::chains::TxStatus;

#[component]
pub fn TxStatusBadge(status: TxStatus) -> Element {
    let (label, color) = match status {
        TxStatus::Pending => ("pending", "#999"),
        TxStatus::Confirmed => ("confirmed", "#7CFC90"),
        TxStatus::Failed => ("failed", "#ff77aa"),
    };

    rsx! {
        span {
            style: "font-size: 12px; font-family: var(--font-mono); color: {color}; border: 1px solid var(--border); padding: 2px 6px; background: var(--bg);",
            "{label}"
        }
    }
}

