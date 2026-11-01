use super::*;
use crate::resource_reads::render_test::text;
use dioxus::core::{DynamicNode, TemplateNode, VNode};

fn pills(v: &VNode) -> Vec<(String, u32)> {
    fn at(v: &VNode, n: &TemplateNode, out: &mut Vec<(String, u32)>) {
        match n {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let mut reaction = None;
                let mut listeners = 0;
                for attr in attrs.iter() {
                    if let dioxus::core::TemplateAttribute::Dynamic { id } = attr {
                        for a in v.dynamic_attrs[*id].iter() {
                            if a.name == "data-reaction-name"
                                && let AttributeValue::Text(s) = &a.value
                            {
                                reaction = Some(s.clone());
                            }
                            listeners +=
                                usize::from(matches!(a.value, AttributeValue::Listener(_)));
                        }
                    }
                }
                if let Some(name) = reaction {
                    assert_eq!(*tag, "span");
                    assert_eq!(listeners, 0, "reaction reading cannot toggle/open");
                    let digits = text(v)
                        .chars()
                        .rev()
                        .take_while(char::is_ascii_digit)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>();
                    out.push((name, digits.parse().expect("rendered reaction count")));
                }
                for c in children.iter() {
                    at(v, c, out);
                }
            }
            TemplateNode::Dynamic { id } => {
                if let DynamicNode::Fragment(nodes) = &v.dynamic_nodes[*id] {
                    for c in nodes {
                        for r in c.template.roots.iter() {
                            at(c, r, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for r in v.template.roots.iter() {
        at(v, r, &mut out);
    }
    out
}

#[tokio::test]
async fn search_reaction_reading_mounts_passive_fallback_live_updates_and_deleted_owner_refusal() {
    for alias in [
        "search-messages-reactions",
        "search-messages-reactions-narrow",
    ] {
        let mut fixture = VirtualDom::new_with_props(
            |name: &'static str| {
                let state = use_signal(|| {
                    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
                    pages::fixture(&mut s, name);
                    s
                });
                let s = state.read();
                let rendered = results(state, &s);
                let node = rendered.as_ref().unwrap();
                assert_eq!(pills(node), [("thumbsup".into(), 3), ("shipit".into(), 1)]);
                assert!(
                    crate::resource_reads::render_test::button(node, "message-search-hit-1-0")
                        .unwrap()
                        .disabled
                );
                rendered
            },
            alias,
        );
        fixture.rebuild_in_place();
    }
    let h = pages::tests::reactions::reaction_harness().await;
    let mut old = h.state;
    let shell = std::mem::replace(
        &mut *old.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    type Captured = Rc<RefCell<Option<VNode>>>;
    let slot = Slot::default();
    let captured = Captured::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot, captured): (Input, Slot, Captured)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            let rendered = {
                let s = state.read();
                rsx! {div{{results(state,&s)}{controls(state,&s)}}}
            };
            *captured.borrow_mut() = Some(rendered.as_ref().unwrap().clone());
            rendered
        },
        (
            Rc::new(RefCell::new(Some(shell))),
            slot.clone(),
            captured.clone(),
        ),
    );
    dom.rebuild_in_place();
    let state = slot.get().unwrap();
    let before = {
        let s = state.peek();
        (
            s.core.active_channel.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.composer.text.clone(),
            s.search_query.clone(),
            pages::status(&s),
        )
    };
    assert_eq!(
        pills(captured.borrow().as_ref().unwrap()),
        [("thumbsup".into(), 2), ("fixture_unknown".into(), 1)]
    );
    assert!(text(captured.borrow().as_ref().unwrap()).contains(":fixture_unknown:"));
    pages::tests::reactions::shared(state, false, false);
    dom.render_immediate_to_vec();
    assert_eq!(
        pills(captured.borrow().as_ref().unwrap()),
        [("thumbsup".into(), 4)]
    );
    pages::tests::reactions::socket(state, "reaction_added");
    dom.render_immediate_to_vec();
    assert_eq!(
        pills(captured.borrow().as_ref().unwrap()),
        [("thumbsup".into(), 5)]
    );
    pages::tests::reactions::socket(state, "partial");
    dom.render_immediate_to_vec();
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Indexed release"));
    assert!(!text(captured.borrow().as_ref().unwrap()).contains("Later shared body"));
    assert_eq!(
        {
            let s = state.peek();
            (
                s.core.active_channel.clone(),
                s.channel_generation,
                s.thread_generation,
                s.core.composer.text.clone(),
                s.search_query.clone(),
                pages::status(&s),
            )
        },
        before
    );
    pages::tests::reactions::socket(state, "deleted");
    dom.render_immediate_to_vec();
    assert!(pills(captured.borrow().as_ref().unwrap()).is_empty());
    assert!(text(captured.borrow().as_ref().unwrap()).contains("This message was deleted."));
    let mut state = state;
    let epoch = state.peek().core.account_epoch;
    state.write().core.account_epoch = epoch.wrapping_add(1);
    dom.render_immediate_to_vec();
    assert!(pills(captured.borrow().as_ref().unwrap()).is_empty());
    assert!(text(captured.borrow().as_ref().unwrap()).contains("Press Enter to refresh"));
}

#[tokio::test]
async fn search_reaction_groups_mount_fallback_variants_live_removal_empty_and_overflow_without_actions()
 {
    for alias in [
        "search-messages-reaction-groups",
        "search-messages-reaction-groups-narrow",
    ] {
        let mut dom = VirtualDom::new_with_props(
            |alias: &'static str| {
                let state = use_signal(|| {
                    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
                    pages::fixture(&mut s, alias);
                    s
                });
                let rendered = results(state, &state.read());
                assert_eq!(
                    pills(rendered.as_ref().unwrap()),
                    [("thumbsup".into(), 6), ("shipit".into(), 1)]
                );
                assert!(
                    crate::resource_reads::render_test::button(
                        rendered.as_ref().unwrap(),
                        "message-search-hit-1-0"
                    )
                    .unwrap()
                    .disabled
                );
                rendered
            },
            alias,
        );
        dom.rebuild_in_place();
    }
    let h = pages::tests::reactions::grouping_harness().await;
    let mut old = h.state;
    let shell = std::mem::replace(
        &mut *old.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    type Captured = Rc<RefCell<Option<VNode>>>;
    let slot = Slot::default();
    let captured = Captured::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot, captured): (Input, Slot, Captured)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            let rendered = {
                let s = state.read();
                rsx! {div{{results(state,&s)}{controls(state,&s)}}}
            };
            *captured.borrow_mut() = Some(rendered.as_ref().unwrap().clone());
            rendered
        },
        (
            Rc::new(RefCell::new(Some(shell))),
            slot.clone(),
            captured.clone(),
        ),
    );
    dom.rebuild_in_place();
    let mut state = slot.get().unwrap();
    let before = {
        let s = state.peek();
        (
            s.search_query.clone(),
            pages::status(&s),
            s.core.active_channel.clone(),
            s.channel_generation,
            s.core.composer.text.clone(),
        )
    };
    assert_eq!(
        pills(captured.borrow().as_ref().unwrap()),
        [
            ("thumbsup".into(), 6),
            ("shipit".into(), 0),
            ("+1".into(), 1)
        ]
    );
    let reading = text(captured.borrow().as_ref().unwrap());
    assert!(reading.contains(&super_platinum_core::state::emoji_glyph(
        "thumbsup::skin-tone-2"
    )));
    assert!(reading.contains(&super_platinum_core::state::emoji_glyph(
        "thumbsup::skin-tone-5"
    )));
    let hit = pages::rows(&state.peek())[0].clone();
    let original = pages::reactions(&state.peek(), 0);
    assert_eq!(
        original.len(),
        5,
        "presentation does not rewrite original occurrences"
    );
    assert_eq!(
        original[0]
            .reactors
            .iter()
            .map(|r| r.user_id.as_str())
            .collect::<Vec<_>>(),
        ["U1", "U2"]
    );
    let message = serde_json::from_value(
        json!({"ts":hit.ts,"text":"Shared context stays out of indexed body","reactions":[
            {"name":"thumbsup::skin-tone-5","count":1,"users":["U3"]},
            {"name":"thumbsup::skin-tone-2","count":2,"users":["U1","U2"]}
        ]}),
    )
    .unwrap();
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .entry(hit.channel_id.clone())
        .or_default()
        .upsert(message);
    dom.render_immediate_to_vec();
    assert_eq!(
        pills(captured.borrow().as_ref().unwrap()),
        [("thumbsup".into(), 3)]
    );
    let reading = text(captured.borrow().as_ref().unwrap());
    assert!(reading.contains("Indexed release") && !reading.contains("Shared context stays out"));
    let first = reading
        .find(&super_platinum_core::state::emoji_glyph(
            "thumbsup::skin-tone-5",
        ))
        .unwrap();
    let next = reading
        .find(&super_platinum_core::state::emoji_glyph(
            "thumbsup::skin-tone-2",
        ))
        .unwrap();
    assert!(first < next, "constituents keep accepted row order");
    let generation = state.peek().core.workspaces["T1"].rt_generation;
    let event=super_platinum_core::slack::realtime::parse_event(&json!({"type":"reaction_removed","user":"U3","reaction":"thumbsup::skin-tone-5","item":{"type":"message","channel":hit.channel_id,"ts":hit.ts}}).to_string()).unwrap();
    crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
    dom.render_immediate_to_vec();
    assert_eq!(
        pills(captured.borrow().as_ref().unwrap()),
        [("thumbsup".into(), 2)]
    );
    assert!(!text(captured.borrow().as_ref().unwrap()).contains(
        &super_platinum_core::state::emoji_glyph("thumbsup::skin-tone-5")
    ));
    let variants = (0..12)
        .map(|i| json!({"name":format!("fixture_cluster::variant-{i}"),"count":1,"users":["U1"]}))
        .collect::<Vec<_>>();
    let message = serde_json::from_value(json!({"ts":hit.ts,"reactions":variants})).unwrap();
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut(&hit.channel_id)
        .unwrap()
        .upsert(message);
    dom.render_immediate_to_vec();
    assert_eq!(
        pills(captured.borrow().as_ref().unwrap()),
        [("fixture_cluster".into(), 12)]
    );
    let reading = text(captured.borrow().as_ref().unwrap());
    assert!(
        reading.contains(":fixture_cluster::variant-9:")
            && !reading.contains(":fixture_cluster::variant-10:")
    );
    assert_eq!(
        pages::reactions(&state.peek(), 0).len(),
        12,
        "glyph cap does not truncate source data"
    );
    pages::tests::reactions::shared(state, false, true);
    dom.render_immediate_to_vec();
    assert!(
        pills(captured.borrow().as_ref().unwrap()).is_empty(),
        "actual shared empty remains authoritative"
    );
    assert_eq!(
        {
            let s = state.peek();
            (
                s.search_query.clone(),
                pages::status(&s),
                s.core.active_channel.clone(),
                s.channel_generation,
                s.core.composer.text.clone(),
            )
        },
        before
    );
}
