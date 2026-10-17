use super::*;

pub(super) fn fields(
    state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    view: rename::field_delete::Presentation,
    enabled: bool,
) -> Element {
    rsx! {
        label { r#for: "slack-list-delete-field-choice", "Field to delete" }
        select {
            id: "slack-list-delete-field-choice", value: "{view.column}", disabled: !enabled,
            onchange: move |event| rename::field_delete::choose(state, serial, visit, event.value()),
            for (id, name) in view.choices {
                option { key: "{id}", value: "{id}", selected: id == view.column, "{name}" }
            }
        }
        p { id: "slack-list-delete-field-warning", class: "slack-list-warning",
            "Permanently delete the selected field from every view? This app does not offer Undo for field deletion."
        }
    }
}
