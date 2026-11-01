//! Original input, canonical channel and mounted private-topic behavior.
use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as support;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};
use std::{cell::Cell, rc::Rc};
const T: &str = "TINIT0001";
const C: &str = "CINIT0001";
const RAW: &str = "*Private planning* _quietly_ &amp; <@U_UNKNOWN> :opaque_emoji: <https://private-topic.invalid/a.png|image>";
fn row() -> Value {
    json!({"id":C,"name":"private-planning","is_channel":true,"is_member":true,"is_private":true,"is_group":false,"is_im":false,"is_mpim":false,"is_archived":false,"is_frozen":false,"is_file":false,"properties":{},"topic":{"value":RAW},"purpose":{"value":"PURPOSE FALLBACK TRAP"}})
}
fn assignments(group: Option<Value>) -> Value {
    let mut raw = json!({"ok":true,"assignments":{"channel_tabs_threads":{"group":"on"},"search_normalize_visible_external_users":{"group":"off"},"preserve_omitted_user_change_member_fields":{"group":"on"}}});
    if let Some(group) = group {
        raw["assignments"]["privacy_mode"] = group;
    }
    raw
}
fn install(mut state: Signal<ShellState>) {
    let mut s = state.write();
    crate::app_home::channel_fetches::fixture::install_people(&mut s);
    support::connect(&mut s).unwrap();
}
async fn finish(
    dom: &mut VirtualDom,
    mut state: Signal<ShellState>,
    initial: support::Initialization,
    channel: Value,
) {
    support::provide(dom, Rc::new(support::Renderer::default()));
    support::run_pumped(dom, initial.drain(state))
        .await
        .unwrap();
    let init = json!({"ok":true,"self":{"id":"UINIT0001","name":"fixture-self","team_id":T,"profile":{}},"team":{"id":T,"name":"Private topics"},"workspaces":[{"id":T}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123,"prefs":{}});
    let channels = json!({"ok":true,"channels":{"channels":[channel],"ims":[],"mpims":[],"is_open":[],"has_more_mpdms":false,"visitor_channels":[],"temporary_channel_expirations":{}}});
    support::run_pumped(dom, initial.complete_channels(state, init, channels))
        .await
        .unwrap();
    state.write().refresh_from_core();
    dom.render_immediate_to_vec();
    assert!(
        canonical::observed(&state.peek(), T)
            .unwrap()
            .current_for(&state.peek(), T)
    );
}
async fn renew(
    dom: &mut VirtualDom,
    mut state: Signal<ShellState>,
    group: Option<Value>,
    channel: Value,
) {
    let initial = {
        let mut s = state.write();
        support::header_privacy_input(&mut s)
            .unwrap()
            .complete(&mut s, &assignments(group))
            .unwrap()
    };
    finish(dom, state, initial, channel).await;
}
fn header(state: Signal<ShellState>) -> Element {
    let s = state.read();
    let name = s
        .channels
        .get(s.active_channel)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    rsx! {header{class:"conversation-header",{super::super::super::conversation_header(state,&s,&name,None)}}}
}
fn mount() -> (VirtualDom, Signal<ShellState>) {
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |slot: Slot| {
            let state = use_signal(|| ShellState::fixture(crate::media::MediaRegistry::default()));
            slot.set(Some(state));
            header(state)
        },
        slot.clone(),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap())
}
#[derive(Debug)]
struct Node {
    tag: String,
    attrs: Vec<(String, String)>,
    listener: bool,
    children: Vec<Node>,
    text: String,
}
impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
    fn content(&self) -> String {
        let mut s = self.text.clone();
        for c in &self.children {
            s.push_str(&c.content());
        }
        s
    }
}
fn nodes(dom: &VirtualDom, v: &VNode) -> Vec<Node> {
    fn at(dom: &VirtualDom, v: &VNode, n: &TemplateNode) -> Vec<Node> {
        match n {
            TemplateNode::Text { text } => vec![Node {
                tag: "#text".into(),
                attrs: Vec::new(),
                listener: false,
                children: Vec::new(),
                text: (*text).into(),
            }],
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let mut values = Vec::new();
                let mut listener = false;
                for a in attrs.iter() {
                    match a {
                        TemplateAttribute::Static { name, value, .. } => {
                            values.push(((*name).into(), (*value).into()))
                        }
                        TemplateAttribute::Dynamic { id } => {
                            for a in v.dynamic_attrs[*id].iter() {
                                let value = match &a.value {
                                    AttributeValue::Text(s) => s.clone(),
                                    AttributeValue::Bool(b) => b.to_string(),
                                    AttributeValue::Listener(_) => {
                                        listener = true;
                                        "<listener>".into()
                                    }
                                    _ => "<other>".into(),
                                };
                                values.push((a.name.into(), value));
                            }
                        }
                    }
                }
                vec![Node {
                    tag: (*tag).into(),
                    attrs: values,
                    listener,
                    children: children.iter().flat_map(|n| at(dom, v, n)).collect(),
                    text: String::new(),
                }]
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Text(t) => vec![Node {
                    tag: "#text".into(),
                    attrs: Vec::new(),
                    listener: false,
                    children: Vec::new(),
                    text: t.value.clone(),
                }],
                DynamicNode::Fragment(children) => {
                    children.iter().flat_map(|v| nodes(dom, v)).collect()
                }
                DynamicNode::Component(c) => c
                    .mounted_scope(*id, v, dom)
                    .map(|s| nodes(dom, s.root_node()))
                    .unwrap_or_default(),
                _ => Vec::new(),
            },
        }
    }
    v.template
        .roots
        .iter()
        .flat_map(|n| at(dom, v, n))
        .collect()
}
fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    nodes.iter().find_map(|n| {
        if n.attr("id") == Some(id) {
            Some(n)
        } else {
            find(&n.children, id)
        }
    })
}
fn snapshot(dom: &mut VirtualDom) -> Vec<Node> {
    dom.render_immediate_to_vec();
    nodes(dom, dom.base_scope().root_node())
}
fn inert(n: &Node) {
    assert!(matches!(n.tag.as_str(), "span" | "#text"));
    assert!(!n.listener);
    for (name, value) in &n.attrs {
        assert!(
            !name.starts_with("on")
                && !matches!(
                    name.as_str(),
                    "href" | "src" | "dangerous_inner_html" | "tabindex"
                )
        );
        assert_ne!(value, "<listener>");
        assert_ne!(value, "<other>");
    }
    for c in &n.children {
        inert(c);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn private_topic_original_input_refresh_failure_queued_reply_owner_and_layout() {
    let (mut dom, mut state) = mount();
    install(state);
    renew(&mut dom, state, Some(json!({"group":"off"})), row()).await;
    let receipt =
        crate::app_home::channel_fetches::header_privacy_inactive(&state.peek(), T).unwrap();
    assert!(receipt.current(&state.peek()));
    assert_eq!(project(&state.peek()).unwrap().raw, RAW);
    let tree = snapshot(&mut dom);
    let topic = find(&tree, "conversation-header-topic").unwrap();
    inert(topic);
    assert_eq!(topic.attr("data-topic-provenance"), Some("owned"));
    assert!(
        topic.content().contains("Private planning") && topic.content().contains("& <@U_UNKNOWN>")
    );
    assert!(!topic.content().contains("PURPOSE FALLBACK TRAP"));
    assert!(find(&tree, "conversation-header-private").is_some());
    assert_eq!(
        state
            .peek()
            .media
            .registered_source_count("https://private-topic.invalid/a.png"),
        0
    );
    // A failed original input retires at START, before any response or new slot.
    let failed = support::header_privacy_input(&mut state.write()).unwrap();
    assert!(!receipt.current(&state.peek()));
    assert!(project(&state.peek()).is_none());
    assert!(find(&snapshot(&mut dom), "conversation-header-topic").is_none());
    drop(failed);
    assert!(
        canonical::observed(&state.peek(), T)
            .unwrap()
            .current_for(&state.peek(), T)
    );
    assert!(project(&state.peek()).is_none());
    renew(&mut dom, state, Some(json!({"group":"off"})), row()).await;
    let older = support::header_privacy_input(&mut state.write()).unwrap();
    let newer = support::header_privacy_input(&mut state.write()).unwrap();
    let initial = newer
        .complete(
            &mut state.write(),
            &assignments(Some(json!({"group":"on"}))),
        )
        .unwrap();
    finish(&mut dom, state, initial, row()).await;
    assert!(project(&state.peek()).is_none());
    let initial = older
        .complete(
            &mut state.write(),
            &assignments(Some(json!({"group":"off"}))),
        )
        .unwrap();
    finish(&mut dom, state, initial, row()).await;
    assert!(project(&state.peek()).is_none());
    // Explicit known non-on data is distinct from a missing/default assignment.
    renew(&mut dom, state, Some(json!({"group":"control"})), row()).await;
    assert!(project(&state.peek()).is_some());
    for group in [
        None,
        Some(Value::Null),
        Some(json!({})),
        Some(json!({"group":null})),
        Some(json!({"group":true})),
        Some(json!({"group":"on"})),
        Some(json!({"group":""})),
        Some(json!({"group":"x".repeat(129)})),
    ] {
        renew(&mut dom, state, group, row()).await;
        assert!(project(&state.peek()).is_none());
        assert!(find(&snapshot(&mut dom), "conversation-header-topic").is_none());
    }
    renew(&mut dom, state, Some(json!({"group":"off"})), row()).await;
    let input = support::header_privacy_input(&mut state.write()).unwrap();
    let mut malformed = assignments(Some(json!({"group":"off"})));
    malformed["assignments"]["public_channel_retain_membership_on_archive"] = json!({"group":true});
    assert!(input.complete(&mut state.write(), &malformed).is_err());
    assert!(project(&state.peek()).is_none());
    // Decoder/legacy payload copying cannot bind the original input to a new Arc.
    let initial = support::assignments(
        &mut state.write(),
        &assignments(Some(json!({"group":"off"}))),
    )
    .unwrap();
    finish(&mut dom, state, initial, row()).await;
    assert!(project(&state.peek()).is_none());
    renew(&mut dom, state, Some(json!({"group":"off"})), row()).await;
    state
        .write()
        .core
        .workspaces
        .get_mut(T)
        .unwrap()
        .channels
        .get_mut(C)
        .unwrap()
        .extra
        .insert("topic".into(), json!({"value":"unadmitted replacement"}));
    state.write().refresh_from_core();
    assert!(project(&state.peek()).is_none());
    // Reacquire through a new original fixture owner after the deliberate
    // unpaired native mutation; never manufacture canonical readiness.
    install(state);
    renew(&mut dom, state, Some(json!({"group":"off"})), row()).await;
    let old_receipt =
        crate::app_home::channel_fetches::header_privacy_inactive(&state.peek(), T).unwrap();
    let old_input = support::header_privacy_input(&mut state.write()).unwrap();
    install(state);
    renew(&mut dom, state, Some(json!({"group":"off"})), row()).await;
    assert!(!old_receipt.current(&state.peek()));
    assert!(
        old_input
            .complete(
                &mut state.write(),
                &assignments(Some(json!({"group":"off"})))
            )
            .is_err()
    );
    assert!(project(&state.peek()).is_some());
    inert(find(&snapshot(&mut dom), "conversation-header-topic").unwrap());
    let mut nonmember = row();
    nonmember["is_member"] = json!(false);
    renew(&mut dom, state, Some(json!({"group":"off"})), nonmember).await;
    // The actual joined-channel constructor supplies positive membership;
    // the current processed pair, rather than this input field, is consumed.
    assert_eq!(
        canonical::observed(&state.peek(), T)
            .unwrap()
            .row(C)
            .unwrap()["is_member"],
        json!(true)
    );
    assert_eq!(
        state.peek().core.workspaces[T].channels[C]
            .extra
            .get("is_member"),
        Some(&json!(true))
    );
    assert!(project(&state.peek()).is_some());
    state
        .write()
        .core
        .workspaces
        .get_mut(T)
        .unwrap()
        .channels
        .get_mut(C)
        .unwrap()
        .extra
        .insert("is_member".into(), json!(false));
    state.write().refresh_from_core();
    assert!(project(&state.peek()).is_none());
    install(state);
    let mut public = row();
    public["is_private"] = json!(false);
    renew(&mut dom, state, Some(json!({"group":"on"})), public).await;
    assert!(project(&state.peek()).is_none());
    assert!(super::super::project(&state.peek()).is_some());
    inert(find(&snapshot(&mut dom), "conversation-header-topic").unwrap());
    renew(&mut dom, state, Some(json!({"group":"off"})), row()).await;
    state.write().core.account_epoch += 1;
    assert!(project(&state.peek()).is_none());
    assert!(find(&snapshot(&mut dom), "conversation-header-topic").is_none());
    for alias in [
        "conversation-header-private-topic",
        "conversation-header-private-topic-narrow",
    ] {
        let mut layout = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
        assert!(
            layout.core.session.is_none()
                && layout.core.transport.is_none()
                && layout.core.active_account.is_none()
        );
        assert!(project(&layout).is_none());
        let model = fixture_model(&layout).unwrap();
        assert_eq!(model.provenance, Provenance::Layout);
        let v = render_topic(model).unwrap();
        let tree = nodes(&dom, &v);
        inert(find(&tree, "conversation-header-topic").unwrap());
        layout.core.session = state.peek().core.session.clone();
        assert!(fixture_model(&layout).is_none());
        layout.core.session = None;
        layout.core.transport = state.peek().core.transport.clone();
        assert!(fixture_model(&layout).is_none());
        layout.core.transport = None;
        layout.core.active_account = Some("credential-bearing".into());
        assert!(fixture_model(&layout).is_none());
    }
}
