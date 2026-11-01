//! Accepted boot/member/canonical producer and inert full header DOM.
use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as support;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
const T: &str = "TINIT0001";
const C: &str = "CINIT0001";
const D: &str = "DINIT0001";
const U: &str = "UINIT0001";
const NOW: u64 = 1_800_000_000;
struct Time(Option<u64>);
impl Time {
    fn new(now: u64) -> Self {
        Self(clock::test_now(Some(now)))
    }
}
impl Drop for Time {
    fn drop(&mut self) {
        clock::test_now(self.0);
    }
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
fn profile(emoji: &str, expires: u64) -> Value {
    json!({"display_name":"fixture self","status_text":"coffee break","status_text_canonical":"","status_emoji":emoji,"status_expiration":expires,"statusEmojiDisplayInfo":null})
}
async fn seed(dom: &mut VirtualDom, mut state: Signal<ShellState>, emoji: &str, expires: u64) {
    let p = profile(emoji, expires);
    let user = json!({"id":U,"name":"fixture-self","team_id":T,"deleted":false,"is_bot":false,"profile":p});
    let init = {
        let mut s = state.write();
        crate::app_home::channel_fetches::fixture::install_people(&mut s);
        let transport = s.core.transport.clone().unwrap();
        let session = s.core.session.as_ref().unwrap().workspaces[T].clone();
        let boot = super_platinum_core::state::posting_metadata::authority::BootRead::begin_full(
            &mut s.core,
            &transport,
            &session,
        )
        .unwrap();
        assert!(boot.apply(&mut s.core,json!({"ok":true,"team":{"id":T,"name":"Header statuses"},"self":user,"users":[user],"channels":[{"id":C,"name":"ordinary","is_channel":true,"is_member":true}],"prefs":{}}),true).unwrap());
        support::connect(&mut s).unwrap();
        support::assignments(&mut s,&json!({"ok":true,"assignments":{"channel_tabs_threads":{"group":"on"},"search_normalize_visible_external_users":{"group":"off"},"preserve_omitted_user_change_member_fields":{"group":"on"}}})).unwrap()
    };
    support::provide(dom, Rc::new(support::Renderer::default()));
    support::run_pumped(dom, init.drain(state)).await.unwrap();
    let raw = json!({"ok":true,"self":user,"team":{"id":T,"name":"Header statuses"},"workspaces":[{"id":T}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123,"prefs":{}});
    let channels = json!({"ok":true,"channels":{"channels":[{"id":C,"name":"ordinary","is_channel":true,"is_member":true}],"ims":[{"id":D,"is_im":true,"is_channel":false,"is_mpim":false,"is_group":false,"is_archived":false,"is_user_deleted":false,"user":U,"properties":{}}],"mpims":[],"is_open":[D],"has_more_mpdms":false,"visitor_channels":[],"temporary_channel_expirations":{}}});
    support::run_pumped(dom, init.complete_channels(state, raw, channels))
        .await
        .unwrap();
    state.write().refresh_from_core();
    let index = state
        .peek()
        .channels
        .iter()
        .position(|c| c.id == D)
        .unwrap();
    state
        .write()
        .select_channel(index, crate::state::ChannelOpen::Global);
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

fn status<'a>(tree: &'a [Node]) -> &'a Node {
    find(tree, "conversation-header-status").unwrap()
}
#[tokio::test(flavor = "current_thread")]
async fn dm_header_status_actual_manual_glyph_expiry_current_pair_and_occurrence_isolation() {
    let _time = Time::new(NOW);
    assert_eq!(clock::rounded_millis(1_499), Some(1));
    assert_eq!(clock::rounded_millis(1_500), Some(2));
    assert_eq!(clock::rounded_millis(u128::MAX), None);
    let (mut dom, mut state) = mount();
    assert!(project(&state.peek(), NOW).is_none());
    seed(&mut dom, state, ":coffee:", NOW + 40).await;
    let shown = project(&state.peek(), NOW).unwrap();
    assert_eq!(shown.emoji, ":coffee:");
    assert_eq!(shown.provenance, Provenance::Owned);
    let tree = snapshot(&mut dom);
    let node = status(&tree);
    inert(node);
    assert_eq!(
        node.content(),
        super_platinum_core::state::emoji_glyph("coffee")
    );
    assert_eq!(node.attr("data-status-provenance"), Some("owned"));
    assert!(node.attr("aria-label").unwrap().contains("coffee"));
    assert!(find(&tree, "channel-details-trigger").is_none());
    assert_eq!(
        state.peek().core.workspaces[T].users[U]
            .profile
            .as_ref()
            .unwrap()
            .extra["status_text_canonical"],
        ""
    );
    let start = Instant::now();
    let mut timer = clock::Clock::default();
    assert!(!timer.poll_at(&state.peek(), start, Some(NOW)));
    // Unchanged actual renders cannot postpone the deadline indefinitely.
    drop(state.write());
    snapshot(&mut dom);
    assert!(!timer.poll_at(
        &state.peek(),
        start + Duration::from_secs(29),
        Some(NOW + 29)
    ));
    assert!(!timer.poll_at(
        &state.peek(),
        start + Duration::from_secs(30),
        Some(NOW + 30)
    ));
    clock::test_now(Some(NOW + 60));
    assert!(timer.poll_at(
        &state.peek(),
        start + Duration::from_secs(60),
        Some(NOW + 60)
    ));
    assert!(!timer.poll_at(
        &state.peek(),
        start + Duration::from_secs(60),
        Some(NOW + 60)
    ));
    // Same production silent-tick notification; actual render acknowledges clear.
    drop(state.write());
    assert!(find(&snapshot(&mut dom), "conversation-header-status").is_none());
    assert!(!timer.poll_at(
        &state.peek(),
        start + Duration::from_secs(90),
        Some(NOW + 90)
    ));
    clock::test_now(Some(NOW));
    seed(&mut dom, state, ":rocket:", 0).await;
    let tree = snapshot(&mut dom);
    inert(status(&tree));
    assert_eq!(status(&tree).attr("data-status-emoji"), Some(":rocket:"));
    assert!(!timer.poll_at(
        &state.peek(),
        start + Duration::from_secs(120),
        Some(NOW + 120)
    ));
    // A typed patch cannot borrow old processed-source/profile readiness.
    state
        .write()
        .core
        .workspaces
        .get_mut(T)
        .unwrap()
        .users
        .get_mut(U)
        .unwrap()
        .profile
        .as_mut()
        .unwrap()
        .status_emoji = Some(":coffee:".into());
    assert!(project(&state.peek(), NOW).is_none());
    assert!(find(&snapshot(&mut dom), "conversation-header-status").is_none());
    seed(&mut dom, state, ":coffee:", NOW + 20).await;
    snapshot(&mut dom);
    let original = project(&state.peek(), NOW).unwrap().owner.unwrap();
    let mut old_clock = clock::Clock::default();
    assert!(!old_clock.poll_at(&state.peek(), start, Some(NOW)));
    // Independent equal-ID Shell and actual new control; old clock/drop must not
    // withdraw the successor observation or adopt its read authority.
    let (mut successor, mut successor_state) = mount();
    seed(&mut successor, successor_state, ":rocket:", NOW + 20).await;
    snapshot(&mut successor);
    assert!(!original.context.current(&successor_state.peek()));
    let mut next_clock = clock::Clock::default();
    assert!(!next_clock.poll_at(&successor_state.peek(), start, Some(NOW)));
    assert!(!old_clock.poll_at(
        &state.peek(),
        start + Duration::from_secs(31),
        Some(NOW + 31)
    ));
    drop(dom);
    clock::test_now(Some(NOW + 31));
    assert!(next_clock.poll_at(
        &successor_state.peek(),
        start + Duration::from_secs(31),
        Some(NOW + 31)
    ));
    assert!(!next_clock.poll_at(
        &successor_state.peek(),
        start + Duration::from_secs(31),
        Some(NOW + 31)
    ));
    drop(successor_state.write());
    assert!(find(&snapshot(&mut successor), "conversation-header-status").is_none());
    // One data corpus: unsupported/malformed inputs grant no resources or aliases.
    for p in [
        json!({"status_emoji":":coffee:"}),
        json!({"status_emoji":":coffee:","status_expiration":-1}),
        json!({"status_emoji":":not_a_real_status:","status_expiration":0}),
        json!({"status_emoji":":coffee:","status_expiration":0,"statusEmojiDisplayInfo":{"coffee":{"display_alias":"rocket","display_url":"https://status.invalid/custom.png"}}}),
    ] {
        let native = serde_json::from_value(p.clone()).unwrap();
        assert!(supplied(&p, &native, NOW).is_none());
    }
    let mut p = profile(":coffee:", 0);
    p.as_object_mut().unwrap().remove("statusEmojiDisplayInfo");
    let native = serde_json::from_value(p.clone()).unwrap();
    assert!(supplied(&p, &native, NOW).is_some());
    for id in ["USLACKBOT", "USLACK"] {
        let raw = json!({"id":id,"is_bot":false,"deleted":false,"profile":{}});
        let native = serde_json::from_value(raw.clone()).unwrap();
        assert!(!ordinary_member(&raw, &native));
    }
    let raw = json!({"id":U,"deleted":false,"profile":{}});
    let native = serde_json::from_value(raw.clone()).unwrap();
    assert!(!ordinary_member(&raw, &native));
    assert!(!ordinary_channel(
        &json!({"id":D,"is_im":true,"user_id":U}),
        D,
        U
    ));
    assert_eq!(
        successor_state
            .peek()
            .media
            .registered_source_count("https://status.invalid/custom.png"),
        0
    );
    for name in [
        "conversation-header-status",
        "conversation-header-status-narrow",
    ] {
        let mut layout = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
        assert!(
            layout.core.session.is_none()
                && layout.core.transport.is_none()
                && layout.core.active_account.is_none()
        );
        assert!(project(&layout, NOW).is_none());
        let data = fixture_model(&layout).unwrap();
        assert_eq!(data.provenance, Provenance::Layout);
        assert!(data.owner.is_none());
        let v = render_status(data).unwrap();
        let tree = nodes(&successor, &v);
        inert(status(&tree));
        layout.core.session = successor_state.peek().core.session.clone();
        assert!(fixture_model(&layout).is_none());
        layout.core.session = None;
        layout.core.transport = successor_state.peek().core.transport.clone();
        assert!(fixture_model(&layout).is_none());
        layout.core.transport = None;
        layout.core.active_account = Some("credential-bearing".into());
        assert!(fixture_model(&layout).is_none());
    }
}
