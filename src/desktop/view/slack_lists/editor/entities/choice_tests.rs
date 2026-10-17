use super::*;
use dioxus::core::{ElementId, Mutation, Mutations};
use std::{cell::RefCell, rc::Rc};
type Sink = Rc<RefCell<Vec<(String, String, bool)>>>;
fn node(edits: &Mutations, event: &str) -> ElementId {
    edits
        .edits
        .iter()
        .find_map(|e| match e {
            Mutation::NewEventListener { name, id } if *name == event => Some(*id),
            _ => None,
        })
        .unwrap()
}
#[test]
fn actual_shared_choices_keep_cell_change_semantics_and_creation_single_activation() {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    for (toggle_single, multi) in [(false, false), (false, true), (true, false), (true, true)] {
        let events = Sink::default();
        let mut dom = VirtualDom::new_with_props(
            |(toggle_single, multi, events): (bool, bool, Sink)| {
                let directory = crate::slack_lists::entities::Presentation {
                    candidates: Arc::new(vec![crate::slack_lists::entities::Choice {
                        id: "U00000001".into(),
                        label: "Alice".into(),
                    }]),
                    ..Default::default()
                };
                rsx! {DirectoryChoices{kind:crate::slack_lists::entities::Kind::Users,directory,selected:vec!["U00000001".into()],multi,disabled:false,input_name:"test-owned",toggle_single,activate:move |event|events.borrow_mut().push(event)}}
            },
            (toggle_single, multi, events.clone()),
        );
        let initial = dom.rebuild_to_vec();
        let click = node(&initial, "click");
        let change = node(&initial, "change");
        let payload = dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default());
        dom.runtime().handle_event(
            "click",
            dioxus::core::Event::new(payload.into_any(), true),
            click,
        );
        let payload = dioxus::html::EventData::Form(dioxus::html::SerializedFormData::new(
            "false".into(),
            Vec::new(),
        ));
        dom.runtime().handle_event(
            "change",
            dioxus::core::Event::new(payload.into_any(), true),
            change,
        );
        assert_eq!(
            *events.borrow(),
            vec![("U00000001".into(), "Alice".into(), toggle_single && !multi)]
        );
    }
}
