//! Modal confirmation dialog. Pure presentation: the caller decides when to
//! render it and what confirming does. Clicking the backdrop or pressing
//! Escape cancels.

use dioxus::prelude::*;

#[component]
pub fn ConfirmDialog(
    title: String,
    message: String,
    confirm_label: String,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_cancel.call(()),
            div {
                class: "surface surface-raised surface-blur modal",
                role: "dialog",
                "aria-modal": "true",
                "aria-labelledby": "modal-title",
                // Keep clicks inside the dialog from reaching the backdrop.
                onclick: move |e| e.stop_propagation(),
                onkeydown: move |e| {
                    // The keys are the dialog's: Escape must not also step the
                    // game back behind it.
                    e.stop_propagation();
                    if e.key() == Key::Escape {
                        on_cancel.call(());
                    }
                },
                h2 { id: "modal-title", class: "modal-title", {title} }
                p { class: "modal-message", {message} }
                div { class: "modal-actions",
                    button {
                        class: "button button-secondary modal-btn",
                        // Focus the safe choice so Escape/Enter work at once;
                        // `autofocus` only applies on page load.
                        onmounted: move |e| async move {
                            let _ = e.data().set_focus(true).await;
                        },
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "button button-primary modal-btn modal-btn-primary",
                        onclick: move |_| on_confirm.call(()),
                        {confirm_label}
                    }
                }
            }
        }
    }
}
