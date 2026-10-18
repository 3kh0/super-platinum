//! Actual generated cell-editor input/callback regressions. Native date validity
//! and focus are separately checked by the offline WebView workflow.
use super::*;
use dioxus::core::{AttributeValue, ElementId, Mutation, Mutations};
use std::{cell::Cell, rc::Rc};
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
fn input_id(edits: &Mutations) -> ElementId {
    edits
        .edits
        .iter()
        .find_map(|m| match m {
            Mutation::SetAttribute {
                name: "initial_value",
                value: AttributeValue::Text(value),
                id,
                ..
            } if value == "2026-10-17" => Some(*id),
            _ => None,
        })
        .expect("actual uncontrolled calendar date input")
}
fn harness() -> (VirtualDom, Signal<ShellState>, Mutations) {
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                let mut shell = ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list",
                );
                crate::slack_lists::task_date::fixture::seed(&mut shell, "todo_due_date", true);
                shell
            });
            slot.set(Some(state));
            let view = crate::slack_lists::presentation(&state.read()).unwrap();
            if let Some(editor) = view.editor {
                rsx! {CellEditor {key:"list-cell-editor-{editor.generation}", state,editor,visit:view.generation}}
            } else {
                rsx! {}
            }
        },
        slot.clone(),
    );
    let edits = dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), edits)
}
#[test]
fn special_personal_date_renders_existing_uncontrolled_input_and_typing_never_writes() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let (mut dom, state, edits) = harness();
    let input = input_id(&edits);
    let initial = crate::slack_lists::presentation(&state.peek())
        .unwrap()
        .editor
        .unwrap();
    assert_eq!(initial.kind, EditorKind::Date);
    assert_eq!(
        *initial.original,
        CellEditValue::Date(Some("2026-10-17".into()))
    );
    for value in ["2026-11-02", "", "2026-"] {
        let event = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
            value.into(),
            Vec::new(),
        ));
        dom.runtime().handle_event(
            "input",
            dioxus::core::Event::new(event.into_any(), true),
            input,
        );
        let updates = dom.render_immediate_to_vec();
        assert!(
            !updates
                .edits
                .iter()
                .any(|m| matches!(m, Mutation::SetAttribute{id,name:"value",..} if *id==input))
        );
        let current = crate::slack_lists::presentation(&state.peek()).unwrap();
        assert!(current.mutation.is_none());
        assert_eq!(current.editor.unwrap().original, initial.original);
        assert_eq!(crate::slack_lists::fixture::attempt_count(&state.peek()), 0);
    }
}
#[test]
fn canceled_special_date_unmounts_without_a_write_or_canonical_change() {
    let (mut dom, state, edits) = harness();
    let _input = input_id(&edits);
    let before = crate::slack_lists::presentation(&state.peek()).unwrap();
    let generation = before.editor.unwrap().generation;
    let value = before.rows[0].cells[1].text.clone();
    dom.in_runtime(|| crate::slack_lists::cancel_editor(state, generation));
    let updates = dom.render_immediate_to_vec();
    assert!(
        updates
            .edits
            .iter()
            .any(|m| matches!(m, Mutation::Remove { .. } | Mutation::ReplaceWith { .. }))
    );
    let after = crate::slack_lists::presentation(&state.peek()).unwrap();
    assert!(after.editor.is_none() && after.mutation.is_none());
    assert_eq!(after.rows[0].cells[1].text, value);
    assert_eq!(crate::slack_lists::fixture::attempt_count(&state.peek()), 0);
}
#[test]
fn actual_primary_todo_row_button_opens_saved_reminder_for_the_same_record() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                let mut shell = ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list",
                );
                crate::slack_lists::task_date::fixture::seed(&mut shell, "todo_due_date", true);
                crate::slack_lists::task_date::fixture::personal_route(&mut shell);
                shell
            });
            slot.set(Some(state));
            let view = crate::slack_lists::presentation(&state.read()).unwrap();
            let row = super::super::row::row(state, &view, &view.rows[0], 0, true, 0);
            rsx! {table {tbody {{row}}}}
        },
        slot.clone(),
    );
    let edits = dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let button = edits
        .edits
        .iter()
        .find_map(|m| match m {
            Mutation::SetAttribute {
                name: "aria-label",
                value: AttributeValue::Text(value),
                id,
                ..
            } if value == "Remind me, Review the launch plan" => Some(*id),
            _ => None,
        })
        .expect("actual primary personal To-do reminder action");
    let before = crate::slack_lists::presentation(&state.peek()).unwrap();
    assert!(before.editor.is_none() && !before.rows[0].cells[1].can_edit);
    let payload = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.runtime().handle_event(
        "click",
        dioxus::core::Event::new(payload.into_any(), true),
        button,
    );
    let _ = dom.render_immediate_to_vec();
    let after = crate::slack_lists::presentation(&state.peek()).unwrap();
    assert_eq!(after.detail.unwrap().record_id, "R_DATE");
    assert!(after.editor.is_none() && after.mutation.is_none());
    assert_eq!(after.rows[0].cells[1].text, before.rows[0].cells[1].text);
    let reminder = crate::slack_lists::reminder::presentation(&state.peek()).unwrap();
    assert_eq!(reminder.title, "Review the launch plan");
    assert!(reminder.ready && !reminder.pending);
    assert_eq!(crate::slack_lists::fixture::attempt_count(&state.peek()), 0);
}
