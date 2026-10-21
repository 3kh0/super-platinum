//! Canonical Canvas text stays phrasing content, including consecutive soft breaks.
use crate::{model::RichNode, state::ShellState};
use dioxus::prelude::*;
use super_platinum_core::canvas::edit::text::TextDraft;

pub(super) fn render(seed: &TextDraft, epoch: u64, state: Signal<ShellState>) -> Element {
    rsx! {span {class:"canvas-heading-text",
        for run in &seed.runs {
            span {
                class: format!("{}{}{}{}{}", if run.style.bold {" bold"} else {""}, if run.style.italic {" italic"} else {""}, if run.style.strike {" strike"} else {""}, if run.style.code {" inline-code"} else {""}, if run.style.underline {" canvas-text-underline"} else {""}),
                if let Some(url) = &run.style.link {
                    {crate::view::rich::rich_node(&RichNode::Link {label:run.text.clone(),url:url.clone()},epoch,state)}
                } else {"{run.text}"}
            }
        }
    }}
}
