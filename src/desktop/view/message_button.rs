//! A rendered message button hydrates its app identity without dispatching it.
use dioxus::prelude::*;
use super_platinum_core::block_actions::BlockElementMetadata;

use crate::{message_buttons::source::Source, model::ButtonStyle, state::ShellState};

#[component]
pub(crate) fn MessageAppButton(
    state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    label: String,
    accessibility_label: String,
    url: Option<String>,
    style: ButtonStyle,
) -> Element {
    // Source is an ordinary prop, so explicitly track it rather than reading
    // the whole shell from an effect. ensure() peeks and owns request guards.
    let hydration_revision = state.read().message_buttons.hydration_revision;
    use_effect(use_reactive(
        (&source, &hydration_revision),
        move |(source, _)| {
            crate::message_buttons::hydration::ensure(state, source);
        },
    ));
    let presentation =
        crate::message_buttons::presentation_for_button(&state.read(), &source, &metadata);
    let title = presentation.reason.as_deref().unwrap_or_else(|| {
        if presentation.busy {
            "Submitting…"
        } else if presentation.uncertain {
            "Submission has not been confirmed"
        } else {
            url.as_deref().unwrap_or("App action")
        }
    });
    let hover_source = source.clone();
    rsx! {button {
        class:if presentation.enabled {style.class().to_owned()}else{format!("{} inert",style.class())},
        disabled:!presentation.enabled,title:"{title}",
        "aria-label":"{accessibility_label}","aria-busy":presentation.busy.to_string(),
        onmouseenter:move |_|crate::message_buttons::hydration::retry(state,hover_source.clone()),
        ondoubleclick:move |event|event.stop_propagation(),
        onclick:move |event|{
            event.stop_propagation();
            crate::message_buttons::click(state,source.clone(),metadata.clone());
        },
        "{label}"
    }}
}
