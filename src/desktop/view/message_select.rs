//! Native static selects owned by a canonical message; dispatch stays in inputs.
use dioxus::prelude::*;
use super_platinum_core::block_actions::{BlockElementMetadata, form::OptionSpec};

use crate::{message_buttons::source::Source, state::ShellState};

#[component]
pub(crate) fn MessageStaticSelect(
    state: Signal<ShellState>,
    source: Source,
    metadata: BlockElementMetadata,
    accessibility_label: String,
) -> Element {
    let mounted = dioxus::core::use_hook_with_cleanup(
        crate::message_buttons::inputs::display::Mounted::new,
        crate::message_buttons::inputs::display::Mounted::unmount,
    );
    let hydration_revision = state.read().message_buttons.hydration_revision;
    use_effect(use_reactive(
        (&source, &hydration_revision),
        move |(source, _)| {
            crate::message_buttons::inputs::ensure(state, source.clone());
            crate::message_buttons::hydration::ensure(state, source);
        },
    ));
    let mut presentation =
        crate::message_buttons::inputs::presentation(&state.read(), &source, &metadata);
    presentation.selected = crate::message_buttons::inputs::display_selected(
        &state.read(),
        &source,
        &metadata,
        &mounted,
    );
    let selected = presentation.selected.as_deref();
    let native_value = presentation
        .options
        .iter()
        .position(|option| Some(option.value.as_str()) == selected)
        .map(|index| format!("option-{index}"))
        .unwrap_or_default();
    let title = presentation
        .reason
        .as_deref()
        .unwrap_or(if presentation.busy {
            "Submitting…"
        } else {
            "Choose an option"
        });
    let label = if accessibility_label.trim().is_empty() {
        presentation.placeholder.clone()
    } else {
        accessibility_label
    };
    // The native handler needs only canonical values, not raw option metadata.
    let choices: Vec<String> = presentation
        .options
        .iter()
        .map(|option| option.value.clone())
        .collect();
    let hover_source = source.clone();
    let mut offset = 0;
    let groups: Vec<_> = presentation
        .options
        .chunk_by(|left, right| left.group == right.group)
        .map(|options| {
            let start = offset;
            offset += options.len();
            (options[0].group.as_deref(), start..offset)
        })
        .collect();
    rsx! {select {
        class:"block-select",value:"{native_value}",disabled:!presentation.enabled,
        title:"{title}","aria-label":"{label}","aria-busy":presentation.busy.to_string(),
        "data-block-id":metadata.block_id.as_deref().unwrap_or_default(),
        "data-action-id":metadata.action_id.as_deref().unwrap_or_default(),
        "data-input-revision":presentation.revision.to_string(),
        onmouseenter:move |_|crate::message_buttons::hydration::retry(state,hover_source.clone()),
        onclick:move |event|event.stop_propagation(),
        ondoubleclick:move |event|event.stop_propagation(),
        onchange:move |event|{
            event.stop_propagation();
            if let Some(value)=selection(&event.value(),&choices) {
                crate::message_buttons::inputs::change(state,source.clone(),metadata.clone(),value,mounted.clone());
            }
        },
        option {value:"",selected:selected.is_none(),"{presentation.placeholder}"}
        for (group,indices) in groups {
            if let Some(group)=group {
                optgroup {key:"group-{indices.start}",label:"{group}",
                    for index in indices { {native_option(index,&presentation.options[index],selected)} }
                }
            } else {
                for index in indices { {native_option(index,&presentation.options[index],selected)} }
            }
        }
    }}
}

fn native_option(index: usize, option: &OptionSpec, selected: Option<&str>) -> Element {
    // Native values are positional tokens, never user strings: the empty option
    // and values containing commas remain distinct from the clear placeholder.
    rsx! {option {key:"option-{index}",value:"option-{index}",
    selected:selected==Some(option.value.as_str()),"{option.text}"}}
}

/// Outer None rejects malformed DOM tokens; Some(None) is an intentional clear.
fn selection(token: &str, options: &[String]) -> Option<Option<String>> {
    if token.is_empty() {
        return Some(None);
    }
    let index = token.strip_prefix("option-")?.parse::<usize>().ok()?;
    options.get(index).cloned().map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_tokens_preserve_empty_and_comma_values_and_reject_unknown_choices() {
        let options: Vec<_> = ["", "web,mobile", "option-0"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(selection("", &options), Some(None));
        assert_eq!(selection("option-0", &options), Some(Some(String::new())));
        assert_eq!(
            selection("option-1", &options),
            Some(Some("web,mobile".into()))
        );
        assert_eq!(
            selection("option-2", &options),
            Some(Some("option-0".into()))
        );
        for invalid in ["option-3", "option--1", "web,mobile", "missing", "option-"] {
            assert_eq!(selection(invalid, &options), None);
        }
    }
}
