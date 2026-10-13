//! Native ordinary datepicker: valid changes are confirmed before local commit.
use crate::{
    message_buttons::{inputs, source::Source},
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::block_actions::BlockElementMetadata;

#[component]
pub(crate) fn MessageDatepicker(
    state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    accessibility_label: String,
) -> Element {
    let mounted = dioxus::core::use_hook_with_cleanup(
        inputs::display::Mounted::new,
        inputs::display::Mounted::unmount,
    );
    let hydration_revision = state.read().message_buttons.hydration_revision;
    use_effect(use_reactive(
        (&source, &hydration_revision),
        move |(source, _)| {
            inputs::ensure(state, source.clone());
            crate::message_buttons::hydration::ensure(state, source);
        },
    ));
    let view = inputs::date::presentation(&state.read(), &source, &metadata);
    let selected =
        inputs::display_selected(&state.read(), &source, &metadata, &mounted).unwrap_or_default();
    let label = if accessibility_label.trim().is_empty() {
        view.placeholder.clone()
    } else {
        accessibility_label
    };
    let title = view
        .reason
        .as_deref()
        .unwrap_or(if view.busy { "Submitting…" } else { &label });
    let hover_source = source.clone();
    // Pinned Dioxus input.value is volatile: confirmation/cancellation restores
    // the existing node to the mounted committed date, without replacing focus.
    rsx! {input {
        class:"block-datepicker", r#type:"date", value:"{selected}", required:true,
        min:"0001-01-01", max:"9999-12-31", disabled:!view.enabled,
        title:"{title}", "aria-label":"{label}", "aria-busy":view.busy.to_string(),
        "data-block-id":metadata.block_id.as_deref().unwrap_or_default(),
        "data-action-id":metadata.action_id.as_deref().unwrap_or_default(),
        "data-input-revision":view.revision.to_string(),
        onmouseenter:move |_|crate::message_buttons::hydration::retry(state,hover_source.clone()),
        onclick:move |event|event.stop_propagation(),
        ondoubleclick:move |event|event.stop_propagation(),
        onchange:move |event| {
            event.stop_propagation();
            inputs::date::change(state,source.clone(),metadata.clone(),event.value(),mounted.clone());
        },
    }}
}
