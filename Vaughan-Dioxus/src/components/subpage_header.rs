use dioxus::prelude::*;

/// Settings / History style toolbar (back + title).
#[component]
pub fn SubpageToolbar(title: &'static str, on_back: Callback<()>) -> Element {
    rsx! {
        div { class: "subpage-toolbar",
            button {
                class: "icon-circle-btn",
                onclick: move |_| on_back.call(()),
                "←"
            }
            h1 { "{title}" }
        }
    }
}
