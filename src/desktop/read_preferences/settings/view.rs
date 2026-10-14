use super::*;
#[component]
pub(crate) fn ReadingPreferences(state: Signal<ShellState>) -> Element {
    let visit = use_hook(next_id);
    let mut restore_focus = use_signal(|| None::<String>);
    use_effect(move || open(state, visit));
    use_drop(move || {
        // The old keyed pane can unmount while the shell is borrowed for render.
        dioxus::core::spawn_forever(async move {
            tokio::task::yield_now().await;
            close(state, visit);
        });
    });
    let ready = use_memo(move || presentation(&state.read(), visit).is_some_and(|p| !p.busy));
    use_effect(move || {
        if ready() {
            if let Some(id) = restore_focus.peek().clone() {
                let id = serde_json::json!(id);
                dioxus::document::eval(&format!(
                    r#"requestAnimationFrame(()=>{{const p=document.getElementById('reading-preferences');if(p?.dataset.visit!=='{visit}')return;const e=document.getElementById({id});if(e&&!e.matches(':disabled')&&(document.activeElement===document.body||document.activeElement===e))e.focus({{preventScroll:true}});}})"#
                ));
            }
        }
    });
    let Some(p) = presentation(&state.read(), visit) else {
        return rsx! { p { class: "scheduled-hint", "Reading preferences are unavailable for this workspace." } };
    };
    // Native radio activation changes checked before the asynchronous handler.
    // Recreate its input at each check boundary so the visible choice always
    // represents the canonical snapshot, including an unconfirmed outcome.
    let input_generation = format!("{:?}-{}", p.mode, p.busy);
    rsx! {
        section { id: "reading-preferences", class: "reading-preferences", "data-visit": visit,
            "aria-labelledby": "reading-preferences-heading",
            div { class: "reading-preferences-heading",
                h3 { id: "reading-preferences-heading", "Mark as read" }
                button { id: "reading-preferences-reload", r#type: "button", disabled: !p.can_read,
                    onclick: move |_| {
                        restore_focus.set(Some("reading-preferences-reload".into()));
                        reload(state, visit);
                    },
                    if p.uncertain { "Check status" } else { "Reload" }
                }
            }
            p { id: "reading-preferences-status", class: "scheduled-hint", role: "status",
                if p.busy { "Checking reading preferences…" }
                else if let Some(message) = p.message { "{message}" }
                else { "Choose what happens when you open a conversation." }
            }
            fieldset { class: "reading-preferences-options", disabled: !p.can_change,
                legend { "When I open a conversation" }
                for (value, mode, label) in [
                    ("left-off", ReadMode::LeftOffAndRead, "Start where I left off, and mark the conversation as read"),
                    ("newest-read", ReadMode::NewestAndRead, "Start at the newest message, and mark the conversation as read"),
                    ("newest-unseen", ReadMode::NewestKeepUnseen, "Start at the newest message, and leave unseen messages unread"),
                ] {
                    label { key: "{value}-{input_generation}", class: "reading-preferences-option",
                        input { id: "reading-preference-{value}", r#type: "radio", name: "reading-preference",
                            value, checked: p.mode == Some(mode),
                            onchange: move |_| {
                                restore_focus.set(Some(format!("reading-preference-{value}")));
                                change(state, visit, mode);
                            },
                        }
                        span { "{label}" }
                    }
                }
            }
        }
    }
}
