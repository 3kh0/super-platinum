use super::*;
use dioxus::core::{ElementId, Mutation, Mutations, ScopeId};
use std::cell::Cell;
use super_platinum_core::slack_lists::NumberCreatePrecision;
fn event(dom: &VirtualDom, id: ElementId, name: &str, value: &str) {
    let payload = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        value.into(),
        Vec::new(),
    ));
    dom.runtime()
        .handle_event(name, dioxus::core::Event::new(payload.into_any(), true), id);
}
fn listeners(edits: &Mutations, name: &str) -> Vec<ElementId> {
    edits
        .edits
        .iter()
        .filter_map(|e| match e {
            Mutation::NewEventListener { name: n, id } if *n == name => Some(*id),
            _ => None,
        })
        .collect()
}
#[test]
fn actual_panel_exposes_all_six_precisions_and_retains_locked_selected_value() {
    for precision in 0..=5 {
        for locked in [false, true] {
            let result = Rc::new(RefCell::new(None));
            let mut dom = VirtualDom::new_with_props(
                |(result, precision, locked): (Rc<RefCell<Option<Node>>>, u8, bool)| {
                    let state = use_signal(|| {
                        ShellState::fixture_variant(
                            crate::media::MediaRegistry::default(),
                            "slack-list-number-field-create",
                        )
                    });
                    let mut view = rename::presentation(&state.peek()).unwrap();
                    view.field_choice.as_mut().unwrap().number_precision =
                        NumberCreatePrecision::capture(precision).unwrap();
                    if locked {
                        view.stage = Stage::Uncertain;
                        view.can_confirm = false;
                    }
                    let rendered = render_panel(state, view);
                    *result.borrow_mut() = Some(Node {
                        children: expand(rendered.as_ref().unwrap()),
                        ..Default::default()
                    });
                    assert!(rename::text_create::number::close_fixture(&state.peek()));
                    rendered
                },
                (result.clone(), precision, locked),
            );
            dom.rebuild_to_vec();
            let tree = result.borrow_mut().take().unwrap();
            let selector = tree.all("id", "slack-list-new-number-precision")[0];
            assert_eq!(selector.attr("value"), precision.to_string());
            assert_eq!(selector.attr("disabled"), locked.to_string());
            assert_eq!(selector.children.len(), 6);
            for (n, (option, label)) in selector
                .children
                .iter()
                .zip(["None", ".0", ".00", ".000", ".0000", ".00000"])
                .enumerate()
            {
                assert_eq!(option.attr("value"), n.to_string());
                assert_eq!(option.text(), label);
                assert_eq!(
                    option.attr("selected"),
                    (usize::from(precision) == n).to_string()
                );
            }
            assert!(tree.text().contains(&format!(
                "Number format: {precision} decimal places. Missing values stay empty."
            )));
        }
    }
}
#[test]
fn actual_same_render_name_and_precision_callbacks_obey_epoch_and_reset_contract() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| {
                ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list-number-field-create",
                )
            });
            slot.set(Some(state));
            panel(state)
        },
        slot.clone(),
    );
    let initial = dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let changes = listeners(&initial, "change");
    assert_eq!(changes.len(), 2);
    let names = listeners(&initial, "input");
    assert_eq!(names.len(), 1);
    let name = names[0];
    let kind = changes[0];
    let precision = changes[1];
    event(&dom, name, "input", "A");
    event(&dom, name, "input", "Amount");
    assert_eq!(rename::presentation(&state.peek()).unwrap().name, "Amount");
    let before = rename::presentation(&state.peek())
        .unwrap()
        .field_choice
        .unwrap();
    event(&dom, precision, "change", "3");
    let selected = rename::presentation(&state.peek())
        .unwrap()
        .field_choice
        .unwrap();
    assert_eq!(selected.number_precision.value(), 3);
    assert_ne!(selected.revision, before.revision);
    event(&dom, name, "input", "stale");
    event(&dom, precision, "change", "2");
    event(&dom, kind, "change", "text");
    assert_eq!(rename::presentation(&state.peek()).unwrap().name, "Amount");
    assert_eq!(
        rename::presentation(&state.peek()).unwrap().field_choice,
        Some(selected)
    );
    dom.mark_dirty(ScopeId::ROOT);
    let edits = dom.render_immediate_to_vec();
    assert!(!edits.edits.iter().any(|e|matches!(e,Mutation::Remove{id}|Mutation::ReplaceWith{id,..} if *id==name||*id==precision)));
    event(&dom, kind, "change", "number");
    assert_eq!(
        rename::presentation(&state.peek()).unwrap().field_choice,
        Some(selected)
    );
    event(&dom, name, "input", "N");
    event(&dom, name, "input", "Number");
    assert_eq!(rename::presentation(&state.peek()).unwrap().name, "Number");
    for n in 0..=5 {
        event(&dom, precision, "change", &n.to_string());
        assert_eq!(
            rename::presentation(&state.peek())
                .unwrap()
                .field_choice
                .unwrap()
                .number_precision
                .value(),
            n
        );
        dom.mark_dirty(ScopeId::ROOT);
        dom.render_immediate_to_vec();
    }
    event(&dom, kind, "change", "text");
    dom.mark_dirty(ScopeId::ROOT);
    dom.render_immediate_to_vec();
    event(&dom, kind, "change", "number");
    let reset = rename::presentation(&state.peek())
        .unwrap()
        .field_choice
        .unwrap();
    assert_eq!(reset.number_precision.value(), 0);
    assert_ne!(reset.revision, before.revision);
    dom.in_runtime(|| assert!(rename::text_create::number::close_fixture(&state.peek())));
}
