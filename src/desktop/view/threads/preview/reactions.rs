use super::*;

fn labels(v: &VNode) -> Vec<String> {
    let mut out = v
        .dynamic_attrs
        .iter()
        .flat_map(|attrs| attrs.iter())
        .filter_map(|a| {
            if a.name == "aria-label"
                && let AttributeValue::Text(s) = &a.value
                && s.ends_with(" reactions")
            {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for node in v.dynamic_nodes.iter() {
        if let DynamicNode::Fragment(nodes) = node {
            for node in nodes.iter() {
                out.extend(labels(node));
            }
        }
    }
    out.sort();
    out
}
fn reaction(channel: &str, ts: &str, added: bool) -> super_platinum_core::slack::events::RtEvent {
    super_platinum_core::slack::realtime::parse_event(&serde_json::json!({"type":if added {"reaction_added"}else{"reaction_removed"},"user":"U3","reaction":"thumbsup","item":{"type":"message","channel":channel,"ts":ts}}).to_string()).unwrap()
}

#[test]
fn actual_ordinary_root_counts_survive_partial_parent_and_socket_changes_without_new_actions() {
    for alias in [
        "thread-feed-root-reactions",
        "thread-feed-root-reactions-narrow",
    ] {
        let (mut dom, mut state, captured) = mount(alias, render);
        let (channel, root, generation) = {
            let s = state.peek();
            let i = &s.core.threads_view.items[0];
            (
                i.root_msg.channel.clone().unwrap(),
                i.root_msg.ts.clone().unwrap(),
                s.core.workspaces["T1"].rt_generation,
            )
        };
        let key = ("T1".into(), channel.clone(), root.clone());
        let history_before = format!("{:?}", state.peek().core.threads);
        let before = {
            let s = state.peek();
            (
                s.core.composer.text.clone(),
                s.core.thread_composer.text.clone(),
                s.core.active_channel.clone(),
                s.channel_generation,
                s.thread_generation,
                s.core.thread_cursor(&key).map(str::to_owned),
                s.core.pending_marks.len(),
            )
        };
        dom.render_immediate_to_vec();
        assert_eq!(format!("{:?}", state.peek().core.threads), history_before);
        assert_eq!(
            labels(captured.borrow().as_ref().unwrap()),
            vec!["shipit: 1 reactions", "thumbsup: 2 reactions"]
        );
        assert!(text(captured.borrow().as_ref().unwrap()).contains(":shipit:"));
        let page = shape(captured.borrow().as_ref().unwrap(), false);
        assert_eq!(page.actions, 3);
        assert_eq!(
            page.tags.iter().filter(|t| t.as_str() == "button").count(),
            3,
            "only original refresh/open/older buttons"
        );
        for _ in 0..2 {
            crate::realtime::apply_in_view(
                &mut state.write().core,
                "T1",
                generation,
                reaction(&channel, &root, true),
                false,
            );
        }
        dom.render_immediate_to_vec();
        assert_eq!(
            labels(captured.borrow().as_ref().unwrap()),
            vec!["shipit: 1 reactions", "thumbsup: 3 reactions"]
        );
        crate::realtime::apply_in_view(
            &mut state.write().core,
            "T1",
            generation + 1,
            reaction(&channel, &root, false),
            false,
        );
        dom.render_immediate_to_vec();
        assert_eq!(
            labels(captured.borrow().as_ref().unwrap()),
            vec!["shipit: 1 reactions", "thumbsup: 3 reactions"]
        );
        let partial=super_platinum_core::slack::realtime::parse_event(&serde_json::json!({"type":"message","subtype":"message_replied","channel":channel,"message":{"type":"message","channel":channel,"user":"U1","ts":root,"text":"Partial parent keeps reactions","reply_count":148,"latest_reply":"1719800405.000000"}}).to_string()).unwrap();
        crate::realtime::apply_in_view(&mut state.write().core, "T1", generation, partial, false);
        dom.render_immediate_to_vec();
        assert!(
            text(captured.borrow().as_ref().unwrap()).contains("Partial parent keeps reactions")
        );
        assert_eq!(
            labels(captured.borrow().as_ref().unwrap()),
            vec!["shipit: 1 reactions", "thumbsup: 3 reactions"]
        );
        crate::realtime::apply_in_view(
            &mut state.write().core,
            "T1",
            generation,
            reaction(&channel, &root, false),
            false,
        );
        dom.render_immediate_to_vec();
        assert_eq!(
            labels(captured.borrow().as_ref().unwrap()),
            vec!["shipit: 1 reactions", "thumbsup: 2 reactions"]
        );
        {
            let s = state.peek();
            assert_eq!(
                (
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
            assert_eq!(shape(captured.borrow().as_ref().unwrap(), false).actions, 3);
        }
        state.write().core.threads_view.items[0].root_msg.team = Some("OTHER".into());
        dom.render_immediate_to_vec();
        assert!(labels(captured.borrow().as_ref().unwrap()).is_empty());
        state.write().core.threads_view.items[0].root_msg.team = Some("T1".into());
        dom.render_immediate_to_vec();
        assert_eq!(labels(captured.borrow().as_ref().unwrap()).len(), 2);
        state.write().core.account_epoch += 1;
        dom.render_immediate_to_vec();
        assert!(labels(captured.borrow().as_ref().unwrap()).is_empty());
    }
}
