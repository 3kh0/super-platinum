//! Expand the actual renderer's generated RSX, including the destructive action.
use super::*;

#[test]
fn deletion_panel_keeps_explicit_permanent_confirmation_and_locks_target_after_admission() {
    for stage in [
        Stage::Editing,
        Stage::Preparing,
        Stage::Saving,
        Stage::Uncertain,
        Stage::Checking,
        Stage::Conflict,
        Stage::Observed,
    ] {
        let result = Rc::new(RefCell::new(None));
        let mut dom = VirtualDom::new_with_props(
            |(result, stage): (Rc<RefCell<Option<Node>>>, Stage)| {
                let state = use_signal(|| {
                    ShellState::fixture_variant(
                        crate::media::MediaRegistry::default(),
                        "slack-list-select-field-create",
                    )
                });
                let mut view = rename::presentation(&state.peek()).unwrap();
                view.stage = stage;
                view.text_creation = false;
                view.field_choice = None;
                view.select_draft = None;
                view.creation = None;
                view.view = None;
                view.column = "HIDDEN".into();
                view.before = "Hidden field".into();
                view.deletion = Some(rename::field_delete::Presentation {
                    column: "HIDDEN".into(),
                    choices: vec![
                        ("HIDDEN".into(), "Hidden field".into()),
                        ("ABSENT".into(), "Absent field".into()),
                    ],
                });
                view.can_confirm = stage == Stage::Editing;
                view.checkable = matches!(stage, Stage::Uncertain | Stage::Conflict);
                view.current_review = matches!(stage, Stage::Conflict | Stage::Observed);
                let rendered = render_panel(state, view);
                *result.borrow_mut() = Some(Node {
                    children: expand(rendered.as_ref().unwrap()),
                    ..Default::default()
                });
                assert!(rename::text_create::select::close_fixture(&state.peek()));
                rendered
            },
            (result.clone(), stage),
        );
        dom.rebuild_to_vec();
        let tree = result.borrow_mut().take().unwrap();
        let panel = tree.all("id", "slack-list-rename")[0];
        assert_eq!(panel.attr("role"), "dialog");
        assert_eq!(panel.attr("aria-modal"), "true");
        assert_eq!(panel.attr("data-delete-column"), "HIDDEN");
        assert_eq!(
            panel.all("id", "slack-list-rename-title")[0].text(),
            "Delete field"
        );
        assert!(panel.all("id", "slack-list-rename-name").is_empty());
        let chooser = panel.all("id", "slack-list-delete-field-choice")[0];
        assert_eq!(chooser.attr("value"), "HIDDEN");
        assert_eq!(
            chooser.attr("disabled"),
            (stage != Stage::Editing).to_string()
        );
        assert!(chooser.text().contains("Hidden field"));
        assert!(chooser.text().contains("Absent field"));
        assert!(
            panel.all("id", "slack-list-delete-field-warning")[0]
                .text()
                .contains("Permanently delete the selected field from every view? This app does not offer Undo for field deletion.")
        );
        let confirm = panel.all("id", "slack-list-rename-confirm");
        if stage == Stage::Editing {
            assert_eq!(confirm.len(), 1);
            assert_eq!(confirm[0].text(), "Delete field permanently");
            assert_eq!(confirm[0].attr("class"), "danger");
            assert_eq!(confirm[0].attr("disabled"), "false");
            assert_eq!(
                panel.all("id", "slack-list-rename-close")[0].text(),
                "Cancel"
            );
        } else {
            assert!(confirm.is_empty());
        }
        let check = panel.all("id", "slack-list-rename-check");
        if matches!(stage, Stage::Uncertain | Stage::Conflict) {
            assert_eq!(check[0].text(), "Check current fields");
        } else {
            assert!(check.is_empty());
        }
    }
}

fn listener(v: &VNode) -> Option<AttributeValue> {
    v.template.roots.iter().find_map(|n| at(v, n))
}
fn at(v: &VNode, n: &TemplateNode) -> Option<AttributeValue> {
    match n {
        TemplateNode::Element {
            attrs, children, ..
        } => {
            let mut confirm = false;
            let mut click = None;
            for a in attrs.iter() {
                match a {
                    TemplateAttribute::Static {
                        name: "id",
                        value: "slack-list-rename-confirm",
                        ..
                    } => confirm = true,
                    TemplateAttribute::Dynamic { id } => {
                        for a in v.dynamic_attrs[*id].iter() {
                            if a.name == "id"
                                && matches!(&a.value,AttributeValue::Text(t) if t=="slack-list-rename-confirm")
                            {
                                confirm = true;
                            }
                            if a.name == "onclick" {
                                click = Some(a.value.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            if confirm {
                return click;
            }
            children.iter().find_map(|n| at(v, n))
        }
        TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
            DynamicNode::Fragment(v) => v.iter().find_map(listener),
            _ => None,
        },
        _ => None,
    }
}

#[test]
fn generated_chooser_retires_old_confirm_and_remounts_owned_focus_root() {
    use dioxus::core::{Mutation, ScopeId};
    use std::cell::Cell;
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let old_click = Rc::new(RefCell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |(slot, old_click): (Slot, Rc<RefCell<Option<AttributeValue>>>)| {
            let state = use_signal(rename::field_delete::test_shell);
            slot.set(Some(state));
            let rendered = panel(state);
            if old_click.borrow().is_none() {
                *old_click.borrow_mut() = listener(rendered.as_ref().unwrap());
            }
            rendered
        },
        (slot.clone(), old_click.clone()),
    );
    let initial = dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    let before = rename::presentation(&state.peek()).unwrap();
    let target = before
        .deletion
        .as_ref()
        .unwrap()
        .choices
        .iter()
        .find(|c| c.0 != before.column)
        .unwrap()
        .0
        .clone();
    let change = initial
        .edits
        .iter()
        .find_map(|e| match e {
            Mutation::NewEventListener { name, id } if name == "change" => Some(*id),
            _ => None,
        })
        .unwrap();
    let mounted = initial
        .edits
        .iter()
        .find_map(|e| match e {
            Mutation::NewEventListener { name, id } if name == "mounted" => Some(*id),
            _ => None,
        })
        .unwrap();
    let data = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
        target.clone(),
        Vec::new(),
    ));
    dom.runtime().handle_event(
        "change",
        dioxus::core::Event::new(data.into_any(), true),
        change,
    );
    let after = rename::presentation(&state.peek()).unwrap();
    assert_ne!(after.serial, before.serial);
    assert_eq!(after.column, target);
    let AttributeValue::Listener(callback) = old_click.borrow_mut().take().unwrap() else {
        panic!()
    };
    let data = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
    dom.in_runtime(|| callback.call(dioxus::core::Event::new(data.into_any(), true)));
    assert_eq!(
        rename::presentation(&state.peek()).unwrap().stage,
        Stage::Editing
    );
    dom.mark_dirty(ScopeId::ROOT);
    let edits = dom.render_immediate_to_vec();
    assert!(edits.edits.iter().any(|e| matches!(e, Mutation::Remove { id } | Mutation::ReplaceWith { id, .. } if *id == mounted)), "serial change removes the old focus root");
    assert!(
        edits.edits.iter().any(|e| matches!(
            e,
            Mutation::NewEventListener { name, .. } if name == "mounted"
        )),
        "new root re-runs the actual focus installation"
    );
    assert!(state.peek().core.session.is_none() && state.peek().core.transport.is_none());
    dom.in_runtime(|| rename::close(state, after.serial));
    assert!(rename::presentation(&state.peek()).is_none());
}
