//! Genuine current private/legacy/public header reading; no native acceptance.
use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as support;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};
const T: &str = "TINIT0001";
const C: &str = "CINIT0001";
fn row(name: &str) -> Value {
    json!({"id":C,"name":name,"is_channel":true,"is_member":true,"is_private":true,"is_group":false,"is_im":false,"is_mpim":false,"is_archived":false,"is_frozen":false,"is_file":false,"properties":{},"topic":{"value":"ordinary topic"}})
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
async fn seed(dom: &mut VirtualDom, mut state: Signal<ShellState>, channel: Value) {
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
    let init = json!({"ok":true,"self":{"id":"UINIT0001","name":"fixture-self","team_id":T,"profile":{}},"team":{"id":T,"name":"Privacy reading"},"workspaces":[{"id":T}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123,"prefs":{}});
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

fn inert_icon(n: &Node) {
    assert!(matches!(n.tag.as_str(), "span" | "svg" | "path"));
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
        inert_icon(c);
    }
}
fn layout_header(alias: String) -> Element {
    let state = use_signal(move || {
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), &alias)
    });
    let s = state.read();
    let private_title = fixture_model(&s).unwrap();
    let channel = s.core.active_channel.clone().unwrap();
    let title = format!("#{}", private_title.label());
    rsx! {super::super::super::channel_details::ChannelDetailsButton{state,channel,title,private_title:Some(private_title)}}
}
#[tokio::test(flavor = "current_thread")]
async fn private_header_cue_actual_private_legacy_public_overrides_currentness_and_layout() {
    let (mut dom, mut state) = mount();
    assert!(project(&state.peek()).is_none());
    seed(&mut dom, state, row("private-room")).await;
    let cue = project(&state.peek()).unwrap();
    assert_eq!(cue.label(), "private-room");
    assert_eq!(cue.provenance(), "owned");
    let tree = snapshot(&mut dom);
    let icon = find(&tree, "conversation-header-private").unwrap();
    inert_icon(icon);
    assert_eq!(icon.attr("data-header-privacy"), Some("owned"));
    let button = find(&tree, "channel-details-trigger").unwrap();
    assert!(button.listener);
    assert_eq!(button.content(), "private-room");
    assert!(
        button
            .attr("aria-label")
            .unwrap()
            .contains("private channel private-room")
    );
    assert!(find(&tree, "conversation-header-topic").is_none());
    // Current typed/native data cannot borrow an original source name/privacy bit.
    state
        .write()
        .core
        .workspaces
        .get_mut(T)
        .unwrap()
        .channels
        .get_mut(C)
        .unwrap()
        .name = Some("unadmitted-name".into());
    state.write().refresh_from_core();
    assert!(project(&state.peek()).is_none());
    assert!(find(&snapshot(&mut dom), "conversation-header-private").is_none());
    let mut legacy = row("legacy-private");
    legacy["is_channel"] = json!(false);
    legacy["is_private"] = json!(false);
    legacy["is_group"] = json!(true);
    seed(&mut dom, state, legacy).await;
    let tree = snapshot(&mut dom);
    assert_eq!(
        find(&tree, "channel-details-trigger").unwrap().content(),
        "legacy-private"
    );
    inert_icon(find(&tree, "conversation-header-private").unwrap());
    let mut public = row("public-room");
    public["is_private"] = json!(false);
    seed(&mut dom, state, public).await;
    let tree = snapshot(&mut dom);
    assert!(project(&state.peek()).is_none());
    assert!(find(&tree, "conversation-header-private").is_none());
    assert_eq!(
        find(&tree, "channel-details-trigger").unwrap().content(),
        "#public-room"
    );
    assert!(find(&tree, "conversation-header-topic").is_some());
    // Source predicates consume these actual lookup results, not typed defaults;
    // unrelated properties and ordinary archive do not change the filled lock.
    let mut ordinary = row("archived-private");
    ordinary.as_object_mut().unwrap().remove("is_frozen");
    ordinary.as_object_mut().unwrap().remove("is_file");
    ordinary["is_archived"] = json!(true);
    ordinary["properties"] = json!({"is_dormant":false,"record_channel":{"record_id":""}});
    seed(&mut dom, state, ordinary).await;
    assert!(project(&state.peek()).is_some());
    inert_icon(find(&snapshot(&mut dom), "conversation-header-private").unwrap());
    for patch in [
        json!({"is_frozen":true}),
        json!({"is_file":true}),
        json!({"properties":{"record_channel":{"record_id":"AC:fixture","record_type":"agent_channel"}}}),
        json!({"properties":[]}),
    ] {
        let mut value = row("unsupported-private");
        value
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        seed(&mut dom, state, value).await;
        assert!(project(&state.peek()).is_none());
        assert!(find(&snapshot(&mut dom), "conversation-header-private").is_none());
    }
    // The raw marker remains a refusal. Genuine named-row processing clears
    // both markers, so its current processed pair has a different contract.
    for marker in ["isUnknown", "isNonExistent"] {
        let mut incoming = row("named-real-private");
        incoming[marker] = json!(true);
        let native: Channel = serde_json::from_value(incoming.clone()).unwrap();
        assert!(!qualified(&incoming, &native), "unprocessed {marker}");
    }
    let mut incoming = row("named-real-private");
    incoming["isUnknown"] = json!(true);
    incoming["isNonExistent"] = json!(true);
    seed(&mut dom, state, incoming).await;
    {
        let s = state.peek();
        let observed = canonical::observed(&s, T).unwrap();
        let processed = observed.row(C).unwrap();
        let native = &s.core.workspaces[T].channels[C];
        for marker in ["isUnknown", "isNonExistent"] {
            assert_eq!(processed[marker], json!(false));
            assert_eq!(native.extra.get(marker), Some(&json!(false)));
        }
        assert!(qualified(processed, native));
        let owned = project(&s).unwrap();
        assert_eq!(owned.label(), "named-real-private");
        assert_eq!(owned.provenance(), "owned");
    }
    inert_icon(find(&snapshot(&mut dom), "conversation-header-private").unwrap());
    seed(&mut dom, state, row("last-private")).await;
    assert!(project(&state.peek()).is_some());
    state.write().core.account_epoch += 1;
    assert!(project(&state.peek()).is_none());
    assert!(find(&snapshot(&mut dom), "conversation-header-private").is_none());
    for alias in [
        "conversation-header-private",
        "conversation-header-private-narrow",
    ] {
        let mut layout = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
        assert!(
            layout.core.session.is_none()
                && layout.core.transport.is_none()
                && layout.core.active_account.is_none()
        );
        assert!(project(&layout).is_none());
        let label = fixture_model(&layout).unwrap().label().to_owned();
        assert_eq!(fixture_model(&layout).unwrap().provenance(), "layout");
        let mut layout_dom = VirtualDom::new_with_props(layout_header, alias.to_owned());
        layout_dom.rebuild_to_vec();
        let tree = snapshot(&mut layout_dom);
        let icon = find(&tree, "conversation-header-private").unwrap();
        inert_icon(icon);
        assert_eq!(icon.attr("data-header-privacy"), Some("layout"));
        assert_eq!(
            find(&tree, "channel-details-trigger").unwrap().content(),
            label
        );
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
