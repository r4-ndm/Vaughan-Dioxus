use dioxus::prelude::*;
use std::time::Duration;

use crate::utils::clipboard::copy_text;

/// Rainbow segmentation matching Vaughan/web `AddressDisplay` / `ColoredAddress`.
#[component]
pub fn ColoredAddressText(address: String, #[props(default = false)] small: bool) -> Element {
    let addr = address.trim();
    let body = addr.strip_prefix("0x").unwrap_or(addr);
    let font_size = if small { "12px" } else { "18px" };
    let letter = if small { "0.01em" } else { "0.02em" };
    if body.len() < 40 {
        return rsx! {
            span { style: "font-family: var(--font-mono); color: #808080; font-size: {font_size};",
                "{addr}"
            }
        };
    }
    let first = &body[0..5];
    let mid1 = &body[5..18];
    let orange = &body[18..23];
    let grey = &body[23..35];
    let purple = &body[35..40];
    rsx! {
        span { style: "font-family: var(--font-mono); font-weight: 500; font-size: {font_size}; letter-spacing: {letter}; word-break: break-all;",
            span { style: "color: #808080;", "0x" }
            span { style: "color: #33cc33;", "{first}" }
            span { style: "color: #808080;", "{mid1}" }
            span { style: "color: #ff9933;", "{orange}" }
            span { style: "color: #808080;", "{grey}" }
            span { style: "color: #b24cff;", "{purple}" }
        }
    }
}

#[component]
pub fn AddressDisplay(address: String) -> Element {
    if address.is_empty() {
        return rsx! {};
    }

    let copied = use_signal(|| false);
    let addr_for_copy = address.clone();

    let on_click = {
        let mut copied = copied;
        move |_| {
            if copy_text(&addr_for_copy).is_err() {
                return;
            }
            copied.set(true);
            let mut c = copied;
            spawn(async move {
                tokio::time::sleep(Duration::from_secs(2)).await;
                c.set(false);
            });
        }
    };

    rsx! {
        div {
            class: "address-display-click",
            style: "display: flex; align-items: center; justify-content: center; gap: 10px; cursor: pointer; padding: 8px 4px; flex-wrap: wrap;",
            onclick: on_click,
            title: "Copy address",
            ColoredAddressText { address: address.clone() }
            span { style: "font-size: 13px; color: var(--muted-foreground); min-width: 3rem;",
                if *copied.read() { "✓" } else { "" }
            }
        }
    }
}
