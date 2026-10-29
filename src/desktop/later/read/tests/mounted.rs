//! Generated/serialized renderer evidence, not native focus/layout acceptance.
use super::*;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode};
use dioxus::prelude::*;
use super_platinum_core::config::{Session, WorkspaceSession};
type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
fn shell(alias: &'static str) -> ShellState {
    let mut shell = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
    shell.core.active_account = Some("isolated-later-read-test".into());
    shell.core.transport = Some(crate::bootstrap::test_support::transport(
        "synthetic-later-read",
    ));
    shell.core.session = Some(Session {
        d_cookie: "synthetic-later-read".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                user_id: shell.core.workspaces["T1"].self_user_id.clone(),
                enterprise_id: None,
                name: "Fixture".into(),
                url: "https://fixture.invalid".into(),
                token: "synthetic-later-read".into(),
            },
        )]
        .into(),
    });
    shell.core.composer.text = "Keep Later draft".into();
    shell
}
type Captured = Rc<RefCell<Option<VNode>>>;
fn mount(alias: &'static str) -> (VirtualDom, Signal<ShellState>, Captured) {
    let slot = Slot::default();
    let captured = Captured::default();
    let mut dom = VirtualDom::new_with_props(
        |(slot, alias, captured): (Slot, &'static str, Captured)| {
            let state = use_signal(|| shell(alias));
            slot.set(Some(state));
            // Inspect the real component body, including its hooks, static template
            // attributes/text and generated listeners. Child components still mount.
            let rendered = crate::view::LaterPanel(crate::view::LaterPanelProps { state });
            *captured.borrow_mut() = Some(rendered.as_ref().unwrap().clone());
            rendered
        },
        (slot.clone(), alias, captured.clone()),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), captured)
}
fn button(v: &VNode, label: &str) -> Option<AttributeValue> {
    fn at(v: &VNode, node: &TemplateNode, label: &str) -> Option<AttributeValue> {
        match node {
            TemplateNode::Element {
                attrs, children, ..
            } => {
                let mut found = false;
                let mut click = None;
                for attr in attrs.iter() {
                    match attr {
                        TemplateAttribute::Static { name, value, .. } => {
                            if *name == "id" && *value == label {
                                found = true;
                            }
                        }
                        TemplateAttribute::Dynamic { id } => {
                            for attr in v.dynamic_attrs[*id].iter() {
                                if attr.name == "id"
                                    && matches!(&attr.value,AttributeValue::Text(value) if value==label)
                                {
                                    found = true;
                                }
                                if attr.name == "onclick" {
                                    click = Some(attr.value.clone());
                                }
                            }
                        }
                    }
                }
                if found {
                    return click;
                }
                children.iter().find_map(|node| at(v, node, label))
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Fragment(nodes) => nodes.iter().find_map(|v| button(v, label)),
                _ => None,
            },
            _ => None,
        }
    }
    v.template.roots.iter().find_map(|node| at(v, node, label))
}
fn has_text(v: &VNode, expected: &str) -> bool {
    fn at(v: &VNode, node: &TemplateNode, expected: &str) -> bool {
        match node {
            TemplateNode::Text { text } => *text == expected,
            TemplateNode::Element { children, .. } => {
                children.iter().any(|node| at(v, node, expected))
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Text(text) => text.value == expected,
                DynamicNode::Fragment(nodes) => nodes.iter().any(|v| has_text(v, expected)),
                _ => false,
            },
        }
    }
    v.template.roots.iter().any(|node| at(v, node, expected))
}
#[test]
fn actual_later_panel_retains_retry_and_continuation_controls_in_normal_and_narrow_states() {
    for alias in ["later-read-recovery", "later-read-recovery-narrow"] {
        let (_dom, state, captured) = mount(alias);
        let rendered = captured.borrow();
        let rendered = rendered.as_ref().unwrap();
        assert!(!state.peek().core.later.visible.is_empty());
        assert!(button(rendered, "later-read-retry").is_some());
        assert!(button(rendered, "later-read-more").is_some());
        assert_eq!(
            state.peek().core.later.next_cursor.as_deref(),
            Some("fixture-retained-continuation")
        );
        assert!(has_text(rendered, "Load more"));
    }
}
#[cfg(feature = "protected-native-fixture")]
#[tokio::test(flavor = "current_thread")]
async fn actual_later_more_and_retry_callbacks_preserve_exact_failed_append_with_disconnected_transport()
 {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let (mut dom, state, captured) = mount("later-read-recovery");
    let before = keys(state);
    for label in ["later-read-more", "later-read-retry"] {
        let generation = state.peek().core.later.generation;
        let callback = button(captured.borrow().as_ref().unwrap(), label)
            .expect("actual rendered control owns a click listener");
        let AttributeValue::Listener(callback) = callback else {
            panic!("not a click listener")
        };
        dom.in_scope(dioxus::core::ScopeId::ROOT, || {
            callback.call(dioxus::core::Event::new(
                dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default())
                    .into_any(),
                true,
            ))
        });
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                dom.render_immediate_to_vec();
                if state.peek().core.later.generation == generation.wrapping_add(1)
                    && !state.peek().core.later.loading
                {
                    break;
                }
                dom.wait_for_work().await;
            }
        })
        .await
        .expect("actual page callback must settle without HTTP capability");
        let shell = state.peek();
        let failed = shell.later_read.failed.as_ref().unwrap();
        assert!(failed.attempt.append);
        assert_eq!(
            failed.attempt.cursor.as_deref(),
            Some("fixture-retained-continuation")
        );
        assert_eq!(keys(state), before);
        assert!(shell.core.later.loaded);
        assert_eq!(shell.core.composer.text, "Keep Later draft");
    }
}

#[component]
fn Starter(start: EventHandler<()>) -> Element {
    use_effect(move || start.call(()));
    VNode::empty()
}
#[tokio::test(flavor = "current_thread")]
async fn accepted_read_outlives_its_panel_scope_and_finishes_without_stranding_loading() {
    type ShowSlot = Rc<Cell<Option<Signal<bool>>>>;
    let slot = Slot::default();
    let show_slot = ShowSlot::default();
    let (release, held) = tokio::sync::oneshot::channel();
    let held = Rc::new(RefCell::new(Some(held)));
    let finished = Rc::new(Cell::new(false));
    type Props = (
        Slot,
        ShowSlot,
        Rc<RefCell<Option<tokio::sync::oneshot::Receiver<()>>>>,
        Rc<Cell<bool>>,
    );
    let mut dom = VirtualDom::new_with_props(
        |(slot, show_slot, held, finished): Props| {
            let state = use_signal(|| shell("later-read-recovery"));
            let show = use_signal(|| true);
            slot.set(Some(state));
            show_slot.set(Some(show));
            let loading = state.read().core.later.loading;
            rsx! {div {"{loading}"} if show() {Starter {start: move |_| {
                let receive = held.borrow_mut().take().unwrap(); let finished = finished.clone();
                let source = frame(state).replace(LaterFilter::Saved);
                dispatch(state, source, move |state, source| async move {
                    run_with(state, source, |_| async move { receive.await.unwrap(); Ok(page(&["after-unmount"], "")) }).await.unwrap();
                    finished.set(true);
                });
            }}}}
        },
        (slot.clone(), show_slot.clone(), held, finished.clone()),
    );
    dom.rebuild_to_vec();
    let state = slot.get().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            dom.render_immediate_to_vec();
            if state.peek().core.later.loading {
                break;
            }
            dom.wait_for_work().await;
        }
    })
    .await
    .unwrap();
    show_slot.get().unwrap().set(false);
    dom.render_immediate_to_vec();
    assert!(state.peek().core.later.loading);
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            dom.render_immediate_to_vec();
            if finished.get() {
                break;
            }
            dom.wait_for_work().await;
        }
    })
    .await
    .unwrap();
    assert!(!state.peek().core.later.loading);
    assert_eq!(keys(state), ["after-unmount"]);
    assert_eq!(state.peek().core.composer.text, "Keep Later draft");
}
