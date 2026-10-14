//! A mounted lane's explicit Add action. Seed preparation belongs to the
//! controller worker; rendering only reads the cheap shared mutation gates.
use super::*;
use crate::slack_lists::create::board as creation;

#[component]
pub(super) fn LaneAdd(
    state: Signal<ShellState>,
    visit: u64,
    layout: u64,
    lane_key: String,
) -> Element {
    let lease = use_hook(creation::Lease::new);
    let retired = lease.clone();
    use_drop(move || retired.retire());
    let Some(label) = creation::lane_label(&state.read(), visit, layout, &lane_key) else {
        return rsx! {};
    };
    let accessible = format!("Add item to {label}");
    let reason = creation::reason(&state.read());
    let title = reason
        .map(str::to_owned)
        .unwrap_or_else(|| accessible.clone());
    let target = lane_key.clone();
    rsx! {
        button {
            r#type: "button",
            class: "slack-list-action icon-btn slack-list-lane-add",
            // Reuse List colors, hover, disabled and focus styles. Pin this
            // icon-only hit area to the shared 32px control-height token.
            style: "width: var(--control-height); height: var(--control-height); min-width: var(--control-height); padding: 0; display: inline-flex; align-items: center; justify-content: center; flex: 0 0 var(--control-height); margin-inline-start: auto;",
            "data-lane-key": lane_key,
            "aria-label": accessible,
            "aria-describedby": "slack-list-create-help",
            title,
            disabled: reason.is_some(),
            onclick: move |_| creation::add_item(state, visit, layout, target.clone(), lease.clone()),
            {icon(Icon::Add, "icon")}
        }
    }
}
