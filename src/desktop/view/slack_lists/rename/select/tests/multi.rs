use super::*;
use super_platinum_core::slack_lists::SelectCreateFormat;

fn change(dom: &VirtualDom, id: ElementId, checked: bool) {
    // Pinned FormData::checked parses the exact value string as a bool.
    let payload = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        checked.to_string(),
        Vec::new(),
    ));
    dom.runtime().handle_event(
        "change",
        dioxus::core::Event::new(payload.into_any(), true),
        id,
    );
}

#[test]
fn actual_cardinality_checkbox_preserves_pending_rows_and_retires_old_render_callbacks() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list-select-field-create",
                )
            });
            slot.set(Some(state));
            let shell = state.read();
            let p = rename::presentation(&shell).unwrap();
            fields(
                state,
                p.serial,
                crate::slack_lists::generation(&shell).unwrap(),
                rename::selection(&shell).unwrap(),
                p.field_choice.unwrap(),
                p.select_draft.unwrap(),
                true,
            )
        },
        slot.clone(),
    );
    let initial = dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let checkbox = node(&initial, "change");
    let text = node(&initial, "input");
    let remove = node(&initial, "click");
    let before = rename::presentation(&state.peek())
        .unwrap()
        .select_draft
        .unwrap();
    input(&dom, text, "bad\nlabel");
    change(&dom, checkbox, true);
    // Both callbacks are still from the original render and must be refused.
    click(&dom, remove);
    input(&dom, text, "late");
    change(&dom, checkbox, false);
    let now = rename::presentation(&state.peek())
        .unwrap()
        .select_draft
        .unwrap();
    assert_eq!(now.format, SelectCreateFormat::Multiple);
    assert_eq!(now.pending, "bad\nlabel");
    assert_eq!(
        now.rows
            .iter()
            .map(|r| (&r.option, r.generation))
            .collect::<Vec<_>>(),
        before
            .rows
            .iter()
            .map(|r| (&r.option, r.generation))
            .collect::<Vec<_>>()
    );
    dom.mark_dirty(ScopeId::ROOT);
    let edits = dom.render_immediate_to_vec();
    assert!(!edits.edits.iter().any(|e|matches!(e,Mutation::Remove{id}|Mutation::ReplaceWith{id,..} if *id==checkbox || *id==text)));
    assert!(edits.edits.iter().any(|e|matches!(e,Mutation::SetAttribute{id,name:"checked",value:AttributeValue::Bool(true),..} if *id==checkbox)));
    change(&dom, checkbox, false);
    assert_eq!(
        rename::presentation(&state.peek())
            .unwrap()
            .select_draft
            .unwrap()
            .format,
        SelectCreateFormat::Single
    );
    dom.mark_dirty(ScopeId::ROOT);
    dom.render_immediate_to_vec();
    input(&dom, text, "R");
    input(&dom, text, "Ready again");
    assert_eq!(
        rename::presentation(&state.peek())
            .unwrap()
            .select_draft
            .unwrap()
            .pending,
        "Ready again"
    );
    dom.in_runtime(|| assert!(rename::text_create::select::close_fixture(&state.peek())));
}
