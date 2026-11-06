//! Modal confirmation dialog. Pure presentation: the caller decides when to
//! render it and what confirming does. Clicking the backdrop or pressing
//! Escape cancels.
//!
//! The keyboard's focus moves in to the safe choice (Cancel), Tab goes round
//! the dialog's buttons, and closing gives the focus back to what had it —
//! or to the first of `return_to` that shows, when that has gone (say, an
//! item of a menu that closed).

use dioxus::prelude::*;

use crate::components::focus_trap::FocusTrap;

#[component]
pub fn ConfirmDialog(
    title: String,
    message: String,
    confirm_label: String,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
    #[props(default)] return_to: &'static [&'static str],
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
                    if e.key() == Key::Named(NamedKey::Escape) {
                        on_cancel.call(());
                    }
                },
                FocusTrap { id: "confirm-dialog", active: true, fallback: return_to,
                    h2 { id: "modal-title", class: "modal-title", {title} }
                    p { class: "modal-message", {message} }
                    div { class: "modal-actions",
                        button {
                            class: "button button-secondary modal-btn",
                            // The safe choice takes the focus, so Escape and
                            // Enter work at once.
                            "data-autofocus": "true",
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
}
