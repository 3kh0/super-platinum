use super::*;

fn counters(v: &VNode) -> std::collections::BTreeMap<String, String> {
    fn attr(v: &VNode, attrs: &[TemplateAttribute], name: &str) -> Option<String> {
        attrs.iter().find_map(|a| match a {
            TemplateAttribute::Static { name: n, value, .. } if *n == name => Some((*value).into()),
            TemplateAttribute::Dynamic { id } => v.dynamic_attrs[*id].iter().find_map(|a| {
                if a.name == name
                    && let AttributeValue::Text(s) = &a.value
                {
                    Some(s.clone())
                } else {
                    None
                }
            }),
            _ => None,
        })
    }
    fn visit(
        v: &VNode,
        node: &TemplateNode,
        inside: bool,
        out: &mut std::collections::BTreeMap<String, String>,
    ) {
        match node {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let container = attr(v, attrs, "class")
                    .is_some_and(|s| s.split_whitespace().any(|s| s == "thread-feed-reactions"));
                let inside = inside || container;
                if inside {
                    assert!(!matches!(*tag, "button" | "a" | "img" | "input" | "form"));
                    for a in attrs.iter() {
                        if let TemplateAttribute::Dynamic { id } = a {
                            for a in v.dynamic_attrs[*id].iter() {
                                assert!(!matches!(&a.value, AttributeValue::Listener(_)));
                                assert!(!matches!(a.name, "href" | "src"));
                            }
                        }
                    }
                    if let Some(name) = attr(v, attrs, "data-reaction-name") {
                        assert!(
                            out.insert(name, attr(v, attrs, "aria-label").unwrap())
                                .is_none()
                        );
                    }
                }
                for n in children.iter() {
                    visit(v, n, inside, out);
                }
            }
            TemplateNode::Dynamic { id } => {
                if let DynamicNode::Fragment(nodes) = &v.dynamic_nodes[*id] {
                    for v in nodes.iter() {
                        for n in v.template.roots.iter() {
                            visit(v, n, inside, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = std::collections::BTreeMap::new();
    for n in v.template.roots.iter() {
        visit(v, n, false, &mut out);
    }
    out
}
fn reaction(channel: &str, user: &str, added: bool) -> super_platinum_core::slack::events::RtEvent {
    super_platinum_core::slack::realtime::parse_event(&json!({"type":if added {"reaction_added"}else{"reaction_removed"},"user":user,"reaction":"thumbsup","item":{"type":"message","channel":channel,"ts":"1719800401.000000"}}).to_string()).unwrap()
}

fn reaction_feedback(v: &VNode) -> std::collections::BTreeMap<String, (bool, String)> {
    fn attr(v: &VNode, attrs: &[TemplateAttribute], name: &str) -> Option<String> {
        attrs.iter().find_map(|a| match a {
            TemplateAttribute::Static { name: n, value, .. } if *n == name => Some((*value).into()),
            TemplateAttribute::Dynamic { id } => v.dynamic_attrs[*id].iter().find_map(|a| {
                if a.name == name
                    && let AttributeValue::Text(value) = &a.value
                {
                    Some(value.clone())
                } else {
                    None
                }
            }),
            _ => None,
        })
    }
    fn visit(
        v: &VNode,
        node: &TemplateNode,
        out: &mut std::collections::BTreeMap<String, (bool, String)>,
    ) {
        match node {
            TemplateNode::Element {
                attrs, children, ..
            } => {
                if let Some(name) = attr(v, attrs, "data-reaction-name") {
                    let description = attr(v, attrs, "aria-description").unwrap();
                    assert_eq!(attr(v, attrs, "title"), Some(description.clone()));
                    let own = attr(v, attrs, "class")
                        .unwrap()
                        .split_whitespace()
                        .any(|s| s == "own");
                    assert!(out.insert(name, (own, description)).is_none());
                }
                for n in children.iter() {
                    visit(v, n, out);
                }
            }
            TemplateNode::Dynamic { id } => {
                if let DynamicNode::Fragment(nodes) = &v.dynamic_nodes[*id] {
                    for v in nodes.iter() {
                        for n in v.template.roots.iter() {
                            visit(v, n, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // Reuse the original no-image/link/listener assertion on the same tree.
    assert!(!counters(v).is_empty());
    let mut out = Default::default();
    for n in v.template.roots.iter() {
        visit(v, n, &mut out);
    }
    out
}

#[test]
fn thread_reaction_people_show_sampled_names_and_current_viewer_then_follow_live_removals_without_media()
 {
    for alias in [
        "thread-feed-reaction-groups-people",
        "thread-feed-reaction-groups-people-narrow",
    ] {
        let (mut dom, mut state, captured) = mount(alias, render_feed);
        let (channel, root, generation) = {
            let s = state.peek();
            let i = &s.core.threads_view.items[0];
            (
                i.root_msg.channel.clone().unwrap(),
                i.root_msg.ts.clone().unwrap(),
                s.core.workspaces["T1"].rt_generation,
            )
        };
        let before = {
            let s = state.peek();
            (
                s.core.composer.text.clone(),
                s.core.thread_composer.text.clone(),
                s.core.active_channel.clone(),
                s.channel_generation,
                s.thread_generation,
                s.core.pending_marks.len(),
            )
        };
        let original = format!("{:?}", state.peek().core.threads_view.items);
        let history = format!("{:?}", state.peek().core.threads);
        let got = reaction_feedback(captured.borrow().as_ref().unwrap());
        assert_eq!(got["thumbsup"], (true, "11 reactions with :thumbsup: (2 emoji variants) · Maya Chen, Jules, Reactor 6, Reactor 7, Reactor 8, Reactor 9 · You reacted".into()));
        assert_eq!(
            got["wave"],
            (
                true,
                "2 reactions with :wave: · You, Maya Chen · You reacted".into()
            )
        );
        assert!(!got["thumbsup"].1.contains("Reactor 10"));
        assert!(!got["thumbsup"].1.contains("other"));
        assert_eq!(
            counters(captured.borrow().as_ref().unwrap())["thumbsup"],
            "thumbsup: 11 reactions"
        );
        dom.render_immediate_to_vec();
        assert_eq!(
            format!("{:?}", state.peek().core.threads_view.items),
            original
        );
        assert_eq!(format!("{:?}", state.peek().core.threads), history);
        // The same raw users cannot identify the viewer across missing or
        // mismatched current native session/team/self identities.
        let session = state.peek().core.session.clone().unwrap();
        for mismatch in 0..4 {
            {
                let mut s = state.write();
                s.core.session = Some(session.clone());
                match mismatch {
                    0 => s.core.session = None,
                    1 => {
                        s.core
                            .session
                            .as_mut()
                            .unwrap()
                            .workspaces
                            .get_mut("T1")
                            .unwrap()
                            .user_id = "OTHER".into()
                    }
                    2 => {
                        s.core
                            .session
                            .as_mut()
                            .unwrap()
                            .workspaces
                            .get_mut("T1")
                            .unwrap()
                            .team_id = "OTHER".into()
                    }
                    _ => s.core.workspaces.get_mut("T1").unwrap().self_user_id = "OTHER".into(),
                }
            }
            dom.render_immediate_to_vec();
            for (own, description) in
                reaction_feedback(captured.borrow().as_ref().unwrap()).values()
            {
                assert!(!own);
                assert!(!description.contains("You reacted"));
            }
            state
                .write()
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .self_user_id = "U0".into();
        }
        state.write().core.session = Some(session);
        dom.render_immediate_to_vec();
        assert!(reaction_feedback(captured.borrow().as_ref().unwrap())["thumbsup"].0);
        for (ts, name) in [
            (root.as_str(), "thumbsup::skin-tone-2"),
            (root.as_str(), "thumbsup::skin-tone-5"),
            ("1719800401.000000", "wave"),
        ] {
            let event=super_platinum_core::slack::realtime::parse_event(&json!({"type":"reaction_removed","user":"U0","reaction":name,"item":{"type":"message","channel":channel,"ts":ts}}).to_string()).unwrap();
            crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
        }
        dom.render_immediate_to_vec();
        let got = reaction_feedback(captured.borrow().as_ref().unwrap());
        assert_eq!(got["thumbsup"], (false,"9 reactions with :thumbsup: (2 emoji variants) · Maya Chen, Jules, Reactor 6, Reactor 7, Reactor 8, Reactor 9".into()));
        assert_eq!(
            got["wave"],
            (false, "1 reaction with :wave: · Maya Chen".into())
        );
        state
            .write()
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .users
            .get_mut("U1")
            .unwrap()
            .profile
            .as_mut()
            .unwrap()
            .display_name = Some("Maya renamed".into());
        dom.render_immediate_to_vec();
        let got = reaction_feedback(captured.borrow().as_ref().unwrap());
        assert!(got["thumbsup"].1.contains("Maya renamed"));
        assert_eq!(got["wave"].1, "1 reaction with :wave: · Maya renamed");
        let s = state.peek();
        assert_eq!(
            s.media
                .registered_source_count("https://fixture.invalid/group-reaction.png"),
            0
        );
        assert_eq!(
            s.media
                .registered_source_count("https://fixture.invalid/reactor-6.png"),
            0
        );
        assert_eq!(
            before,
            (
                s.core.composer.text.clone(),
                s.core.thread_composer.text.clone(),
                s.core.active_channel.clone(),
                s.channel_generation,
                s.thread_generation,
                s.core.pending_marks.len()
            )
        );
    }
}

#[test]
fn actual_parsed_reply_reactions_render_passive_counts_and_retain_socket_read_and_body_guards() {
    for alias in [
        "thread-feed-replies-reactions",
        "thread-feed-replies-reactions-narrow",
    ] {
        let (mut dom, mut state, captured) = mount(alias, render_feed);
        let (channel, key, generation) = {
            let s = state.peek();
            let i = &s.core.threads_view.items[0];
            let c = i.root_msg.channel.clone().unwrap();
            (
                c.clone(),
                ("T1".into(), c, i.root_msg.ts.clone().unwrap()),
                s.core.workspaces["T1"].rt_generation,
            )
        };
        let before = {
            let s = state.peek();
            (
                format!("{:?}", s.core.threads),
                s.core.composer.text.clone(),
                s.core.thread_composer.text.clone(),
                s.core.active_channel.clone(),
                s.channel_generation,
                s.thread_generation,
                s.core.thread_cursor(&key).map(str::to_owned),
                s.core.pending_marks.len(),
            )
        };
        let labels = counters(captured.borrow().as_ref().unwrap());
        assert_eq!(labels.len(), 2);
        assert_eq!(labels["thumbsup"], "thumbsup: 2 reactions");
        assert_eq!(labels["shipit"], "shipit: 1 reactions");
        assert!(text(captured.borrow().as_ref().unwrap()).contains(":shipit:"));
        for _ in 0..2 {
            crate::realtime::apply_in_view(
                &mut state.write().core,
                "T1",
                generation,
                reaction(&channel, "U3", true),
                false,
            );
        }
        dom.render_immediate_to_vec();
        assert_eq!(
            counters(captured.borrow().as_ref().unwrap())["thumbsup"],
            "thumbsup: 3 reactions"
        );
        {
            let s = state.peek();
            let i = &s.core.threads_view.items[0];
            assert_eq!(i.latest_replies[0].reactions[0].count, 3);
            assert_eq!(i.unread_replies[0].reactions[0].count, 3);
        }
        crate::realtime::apply_in_view(
            &mut state.write().core,
            "T1",
            generation + 1,
            reaction(&channel, "U4", true),
            false,
        );
        dom.render_immediate_to_vec();
        assert_eq!(
            counters(captured.borrow().as_ref().unwrap())["thumbsup"],
            "thumbsup: 3 reactions"
        );
        crate::realtime::apply_in_view(
            &mut state.write().core,
            "T1",
            generation,
            reaction(&channel, "U3", false),
            false,
        );
        dom.render_immediate_to_vec();
        assert_eq!(
            counters(captured.borrow().as_ref().unwrap())["thumbsup"],
            "thumbsup: 2 reactions"
        );
        {
            let s = state.peek();
            assert_eq!(
                (
                    format!("{:?}", s.core.threads),
                    s.core.composer.text.clone(),
                    s.core.thread_composer.text.clone(),
                    s.core.active_channel.clone(),
                    s.channel_generation,
                    s.thread_generation,
                    s.core.thread_cursor(&key).map(str::to_owned),
                    s.core.pending_marks.len()
                ),
                before
            );
            assert_eq!(
                s.media
                    .registered_source_count("https://fixture.invalid/reply.png"),
                0
            );
        }
        state.write().core.threads_view.items[0].latest_replies[0].subtype =
            Some("tombstone".into());
        dom.render_immediate_to_vec();
        assert!(counters(captured.borrow().as_ref().unwrap()).is_empty());
        assert!(text(captured.borrow().as_ref().unwrap()).contains("This message was deleted."));
        state.write().core.account_epoch += 1;
        dom.render_immediate_to_vec();
        assert!(counters(captured.borrow().as_ref().unwrap()).is_empty());
    }
}

#[test]
fn thread_reaction_groups_mount_root_reply_variants_updates_and_cap_without_media_or_guard_relaxation()
 {
    for alias in [
        "thread-feed-reaction-groups",
        "thread-feed-reaction-groups-narrow",
    ] {
        let (mut dom, mut state, captured) = mount(alias, render_feed);
        let (channel, root, generation, mut before, mut expected_history) = {
            let s = state.peek();
            let item = &s.core.threads_view.items[0];
            (
                item.root_msg.channel.clone().unwrap(),
                item.root_msg.ts.clone().unwrap(),
                s.core.workspaces["T1"].rt_generation,
                (
                    format!("{:?}", s.core.threads),
                    s.core.composer.text.clone(),
                    s.core.thread_composer.text.clone(),
                    s.core.active_channel.clone(),
                    s.channel_generation,
                    s.thread_generation,
                    s.core.pending_marks.len(),
                ),
                s.core.threads.clone(),
            )
        };
        // Rendering cannot mutate history. The later explicitly injected Core
        // message_replied frame can, so predict its supplied fields independently
        // before applying it, retaining every other existing bag/state field.
        dom.render_immediate_to_vec();
        assert_eq!(format!("{:?}", state.peek().core.threads), before.0);
        let expected_root = expected_history
            .get_mut(&("T1".into(), channel.clone(), root.clone()))
            .unwrap()
            .messages
            .iter_mut()
            .find(|message| message.ts.as_deref() == Some(root.as_str()))
            .unwrap();
        expected_root.kind = Some("message".into());
        expected_root.channel = Some(channel.clone());
        expected_root.text = Some("Grouped root survives partial parent".into());
        expected_root.reply_count = Some(148);
        before.0 = format!("{:?}", expected_history);
        let counts = counters(captured.borrow().as_ref().unwrap());
        assert_eq!(counts.len(), 3);
        assert_eq!(counts["thumbsup"], "thumbsup: 5 reactions");
        assert_eq!(counts["wave"], "wave: 3 reactions");
        assert_eq!(counts["shipit"], "shipit: 1 reactions");
        let reading = text(captured.borrow().as_ref().unwrap());
        for name in [
            "thumbsup::skin-tone-2",
            "thumbsup::skin-tone-5",
            "wave::skin-tone-2",
            "wave::skin-tone-5",
        ] {
            assert!(
                reading.contains(&super_platinum_core::state::emoji_glyph(name)),
                "{name}"
            );
        }
        assert!(
            reading
                .find(&super_platinum_core::state::emoji_glyph(
                    "thumbsup::skin-tone-2"
                ))
                .unwrap()
                < reading
                    .find(&super_platinum_core::state::emoji_glyph(
                        "thumbsup::skin-tone-5"
                    ))
                    .unwrap()
        );
        assert!(reading.contains(":shipit:"));
        assert_eq!(
            state.peek().core.threads_view.items[0]
                .root_msg
                .reactions
                .len(),
            3
        );
        assert_eq!(
            state.peek().core.threads_view.items[0].latest_replies[0]
                .reactions
                .len(),
            2
        );
        for (ts, user, name) in [
            (root.as_str(), "U3", "thumbsup::skin-tone-5"),
            ("1719800401.000000", "U3", "wave::skin-tone-5"),
            ("1719800401.000000", "U2", "wave::skin-tone-5"),
        ] {
            let event=super_platinum_core::slack::realtime::parse_event(&json!({"type":"reaction_removed","user":user,"reaction":name,"item":{"type":"message","channel":channel,"ts":ts}}).to_string()).unwrap();
            crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, event, false);
        }
        dom.render_immediate_to_vec();
        let counts = counters(captured.borrow().as_ref().unwrap());
        assert_eq!(counts["thumbsup"], "thumbsup: 4 reactions");
        assert_eq!(
            counts["wave::skin-tone-2"], "wave::skin-tone-2: 1 reactions",
            "singleton raw-name label remains intact"
        );
        assert!(!counts.contains_key("wave"));
        assert!(!text(captured.borrow().as_ref().unwrap()).contains(
            &super_platinum_core::state::emoji_glyph("wave::skin-tone-5")
        ));
        let partial=super_platinum_core::slack::realtime::parse_event(&json!({"type":"message","subtype":"message_replied","channel":channel,"message":{"type":"message","channel":channel,"user":"U1","ts":root,"text":"Grouped root survives partial parent","reply_count":148}}).to_string()).unwrap();
        crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, partial, false);
        dom.render_immediate_to_vec();
        assert_eq!(
            counters(captured.borrow().as_ref().unwrap())["thumbsup"],
            "thumbsup: 4 reactions"
        );
        let variants=(0..12).map(|i|json!({"name":format!("fixture_root_cluster::variant-{i}"),"count":1,"users":["U1"],"url":"https://fixture.invalid/group-reaction.png"})).collect::<Vec<_>>();
        state.write().core.threads_view.items[0].root_msg.reactions =
            serde_json::from_value(json!(variants)).unwrap();
        dom.render_immediate_to_vec();
        assert_eq!(
            counters(captured.borrow().as_ref().unwrap())["fixture_root_cluster"],
            "fixture_root_cluster: 12 reactions"
        );
        let reading = text(captured.borrow().as_ref().unwrap());
        assert!(
            reading.contains(":fixture_root_cluster::variant-9:")
                && !reading.contains(":fixture_root_cluster::variant-10:")
        );
        assert_eq!(
            state.peek().core.threads_view.items[0]
                .root_msg
                .reactions
                .len(),
            12
        );
        let duplicate = state.peek().core.threads_view.items[0].root_msg.reactions[0].clone();
        state.write().core.threads_view.items[0]
            .root_msg
            .reactions
            .push(duplicate);
        dom.render_immediate_to_vec();
        assert_eq!(
            counters(captured.borrow().as_ref().unwrap())["fixture_root_cluster"],
            "fixture_root_cluster: 11 reactions",
            "both identical names remain refused before grouping"
        );
        {
            let s = state.peek();
            assert_eq!(
                s.media
                    .registered_source_count("https://fixture.invalid/group-reaction.png"),
                0
            );
            assert_eq!(
                (
                    format!("{:?}", s.core.threads),
                    s.core.composer.text.clone(),
                    s.core.thread_composer.text.clone(),
                    s.core.active_channel.clone(),
                    s.channel_generation,
                    s.thread_generation,
                    s.core.pending_marks.len()
                ),
                before
            );
        }
    }
}
