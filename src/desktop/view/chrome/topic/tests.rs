//! Actual canonical producer and full header DOM, not native/layout acceptance.
use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as support;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};
use std::{cell::Cell, rc::Rc};
const T: &str = "TINIT0001";
const C: &str = "CINIT0001";
const OTHER: &str = "CINIT0002";
const RAW: &str = "*Ship* _calmly_ ~old~ `code` &amp; &lt;safe&gt; <@U_UNKNOWN> :opaque_emoji: <https://topic.invalid/resource.png|image>";
fn row(id: &str, name: &str, topic: Value) -> Value {
    json!({"id":id,"name":name,"is_channel":true,"is_member":true,"is_private":false,"is_group":false,"is_im":false,"is_mpim":false,"is_archived":false,"context_team_id":T,"properties":{},"topic":topic,"purpose":{"value":"PURPOSE FALLBACK TRAP"}})
}
fn header(state: Signal<ShellState>) -> Element {
    let s = state.read();
    let name = s
        .channels
        .get(s.active_channel)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    rsx! {header{class:"conversation-header",{super::super::conversation_header(state,&s,&name,None)}}}
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
async fn seed(dom: &mut VirtualDom, mut state: Signal<ShellState>, topic: Value) {
    let initial = {
        let mut s = state.write();
        crate::app_home::channel_fetches::fixture::install_people(&mut s);
        support::connect(&mut s).unwrap();
        support::assignments(&mut s,&json!({"ok":true,"assignments":{"channel_tabs_threads":{"group":"on"},"search_normalize_visible_external_users":{"group":"off"},"preserve_omitted_user_change_member_fields":{"group":"on"}}})).unwrap()
    };
    support::provide(dom, Rc::new(support::Renderer::default()));
    support::run_pumped(dom, initial.drain(state))
        .await
        .unwrap();
    let init = json!({"ok":true,"self":{"id":"UINIT0001","name":"fixture-self","team_id":T,"profile":{}},"team":{"id":T,"name":"Topic reading"},"workspaces":[{"id":T}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123,"prefs":{}});
    let channels = json!({"ok":true,"channels":{"channels":[row(C,"first-topic",topic),row(OTHER,"second-topic",json!({"value":"Other current topic"}))],"ims":[],"mpims":[],"is_open":[],"has_more_mpdms":false,"visitor_channels":[],"temporary_channel_expirations":{}}});
    support::run_pumped(dom, initial.complete_channels(state, init, channels))
        .await
        .unwrap();
    state.write().refresh_from_core();
    assert!(
        canonical::observed(&state.peek(), T)
            .unwrap()
            .current_for(&state.peek(), T)
    );
    dom.render_immediate_to_vec();
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
async fn header_topic_actual_current_raw_public_projection_inert_header_replacement_and_layout_isolation()
 {
    let (mut dom, mut state) = mount();
    assert!(project(&state.peek()).is_none());
    seed(&mut dom, state, json!({"value":RAW})).await;
    let media_before = state
        .peek()
        .media
        .registered_source_count("https://topic.invalid/resource.png");
    let projected = project(&state.peek()).unwrap();
    assert_eq!(projected.raw, RAW);
    assert_eq!(projected.provenance, Provenance::Owned);
    let tree = snapshot(&mut dom);
    let topic = find(&tree, "conversation-header-topic").unwrap();
    inert(topic);
    assert_eq!(topic.attr("data-topic-provenance"), Some("owned"));
    let content = topic.content();
    assert!(content.contains("Ship") && content.contains("calmly") && content.contains("& <safe>"));
    assert!(
        content.contains("<@U_UNKNOWN>")
            && content.contains(":opaque_emoji:")
            && content.contains("<https://topic.invalid/resource.png|image>")
    );
    assert!(!content.contains("PURPOSE FALLBACK TRAP"));
    assert!(find(&tree, "channel-details-trigger").is_some());
    fn huddle(nodes: &[Node]) -> bool {
        nodes.iter().any(|n| {
            n.attr("class").is_some_and(|s| s.contains("huddle-button")) || huddle(&n.children)
        })
    }
    assert!(huddle(&tree));
    assert_eq!(
        state
            .peek()
            .media
            .registered_source_count("https://topic.invalid/resource.png"),
        media_before
    );
    // Strict raw values, never serialized/defaulted model flags, supply profile.
    let ordinary = row(C, "first-topic", json!({"value":RAW}));
    assert!(qualified(&ordinary, C));
    for key in [
        "is_private",
        "is_group",
        "is_im",
        "is_mpim",
        "is_channel",
        "is_member",
        "is_archived",
    ] {
        let mut bad = ordinary.clone();
        bad.as_object_mut().unwrap().remove(key);
        assert!(!qualified(&bad, C), "missing {key}");
    }
    for patch in [
        json!({"is_private":true}),
        json!({"is_file":true}),
        json!({"isUnknown":true}),
        json!({"deleted":true}),
        json!({"properties":[]}),
        json!({"properties":{"record_channel":{}}}),
        json!({"properties":{"huddles":{"is_huddle_channel":true}}}),
        json!({"properties":{"meetings_channel":{"is_meeting_channel":true}}}),
    ] {
        let mut bad = ordinary.clone();
        bad.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert!(!qualified(&bad, C));
    }
    for bad in [
        Value::Null,
        json!("flat"),
        json!({"value":null}),
        json!({"value":1}),
        json!({"value":""}),
        json!({"value":"x".repeat(MAX_BYTES+1)}),
    ] {
        assert!(text(&bad).is_none());
    }
    // A changed native model cannot borrow the old original row or purpose.
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
        .insert(
            "topic".into(),
            json!({"value":"unadmitted native replacement"}),
        );
    state.write().refresh_from_core();
    assert!(project(&state.peek()).is_none());
    assert!(find(&snapshot(&mut dom), "conversation-header-topic").is_none());
    // Renew through genuine original boot/Init, then replace the conversation.
    seed(&mut dom, state, json!({"value":"   "})).await;
    assert_eq!(project(&state.peek()).unwrap().raw, "   ");
    assert_eq!(
        find(&snapshot(&mut dom), "conversation-header-topic")
            .unwrap()
            .content(),
        "   "
    );
    let index = state
        .peek()
        .channels
        .iter()
        .position(|c| c.id == OTHER)
        .unwrap();
    state
        .write()
        .select_channel(index, crate::state::ChannelOpen::Global);
    assert_eq!(project(&state.peek()).unwrap().raw, "Other current topic");
    let tree = snapshot(&mut dom);
    assert_eq!(
        find(&tree, "conversation-header-topic").unwrap().content(),
        "Other current topic"
    );
    state.write().core.account_epoch += 1;
    assert!(project(&state.peek()).is_none());
    assert!(find(&snapshot(&mut dom), "conversation-header-topic").is_none());
    seed(&mut dom, state, json!("flat topic must not be promoted")).await;
    assert!(project(&state.peek()).is_none());
    // Named layout models are disconnected and never satisfy the live reader.
    for name in [
        "conversation-header-topic",
        "conversation-header-topic-narrow",
    ] {
        let mut layout = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
        assert!(
            layout.core.session.is_none()
                && layout.core.transport.is_none()
                && layout.core.active_account.is_none()
        );
        assert!(project(&layout).is_none());
        let vm = fixture_model(&layout).unwrap();
        assert_eq!(vm.provenance, Provenance::Layout);
        let v = render_topic(vm).unwrap();
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
