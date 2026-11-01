//! One actual reading journey: editor identity, human data, face/profile safety.
use super::*;
use dioxus::core::{DynamicNode, TemplateNode};
use super_platinum_core::slack::realtime::{self, RtUpdate};

fn packet() -> ThreadsViewPage {
    serde_json::from_value(json!({"threads":[
        {"root_msg":{"type":"message","channel":"C1","team":"T1","ts":"10.0","thread_ts":"10.0","user":"U1","text":"DOC ROOT","reply_count":3,
            "document_comment":{"most_recent_editor":"U2","authors":["U3"]}},
         "latest_replies":[
            {"type":"message","channel":"C1","team":"T1","thread_ts":"10.0","ts":"11.0","user":"U1","text":"DOC FIRST AUTHOR","document_comment":{"authors":["U3","U2"]}},
            {"type":"message","channel":"C1","team":"T1","thread_ts":"10.0","ts":"12.0","user":"U1","text":"DOC UNKNOWN","document_comment":{"most_recent_editor":"UMISSING","authors":["U3"]}},
            {"type":"message","channel":"C1","team":"T1","thread_ts":"10.0","ts":"13.0","user":"U1","text":"DOC INVALID","document_comment":{"most_recent_editor":true,"authors":["U3"]}}
         ]},
        {"root_msg":{"type":"message","channel":"C1","team":"T1","ts":"20.0","user":"U1","text":"PLAIN ROOT","reply_count":2},
         "latest_replies":[
            {"type":"message","channel":"C1","team":"T1","thread_ts":"20.0","ts":"21.0","user":"U2","text":"Ordinary editor occurrence"},
            {"type":"message","channel":"C1","team":"T1","thread_ts":"20.0","ts":"22.0","user":"U3","text":"Ordinary author occurrence"}
         ]},
        {"root_msg":{"type":"message","channel":"C1","team":"T1","ts":"30.0","user":"USLACKBOT","text":"UNQUALIFIED SYSTEM","reply_count":0,
            "document_comment":{"most_recent_editor":"U2"}},"latest_replies":[]}
    ],"has_more":false})).unwrap()
}
fn node_text(v: &VNode, node: &TemplateNode) -> String {
    match node {
        TemplateNode::Text { text } => (*text).into(),
        TemplateNode::Element { children, .. } => {
            children.iter().map(|n| node_text(v, n)).collect()
        }
        TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
            DynamicNode::Text(t) => t.value.clone(),
            DynamicNode::Fragment(nodes) => nodes.iter().map(text).collect(),
            _ => String::new(),
        },
    }
}
fn images(v: &VNode, node: &TemplateNode) -> usize {
    match node {
        TemplateNode::Element { tag, children, .. } => {
            usize::from(*tag == "img") + children.iter().map(|n| images(v, n)).sum::<usize>()
        }
        TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
            DynamicNode::Fragment(nodes) => nodes
                .iter()
                .map(|v| v.template.roots.iter().map(|n| images(v, n)).sum::<usize>())
                .sum(),
            _ => 0,
        },
        _ => 0,
    }
}
fn article(v: &VNode, marker: &str) -> Option<(String, usize)> {
    fn at(v: &VNode, n: &TemplateNode, marker: &str) -> Option<(String, usize)> {
        match n {
            TemplateNode::Element { tag, children, .. } => {
                let text = node_text(v, n);
                if *tag == "article" && text.contains(marker) {
                    return Some((text, images(v, n)));
                }
                children.iter().find_map(|n| at(v, n, marker))
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Fragment(nodes) => nodes.iter().find_map(|v| article(v, marker)),
                _ => None,
            },
            _ => None,
        }
    }
    v.template.roots.iter().find_map(|n| at(v, n, marker))
}
fn current_article(captured: &Captured, marker: &str) -> (String, usize) {
    article(captured.borrow().as_ref().unwrap(), marker).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn thread_document_authors_mounted_editor_and_first_author_labels_refuse_wrong_profile_and_ready_posting_face()
 {
    let h = harness();
    let mut initial = h.state;
    setup(initial);
    let (connection, initial_updates) = realtime::protected_connection(12);
    {
        let mut s = initial.write();
        let w = s.core.workspaces.get_mut("T1").unwrap();
        w.rt_generation = 12;
        w.rt = super_platinum_core::state::RealtimeStatus::Connected(connection.clone());
        for (id, name) in [
            ("U1", "CACHED POSTER"),
            ("U2", "CACHED EDITOR"),
            ("U3", "CACHED AUTHOR"),
            ("UMISSING", "CACHED UNKNOWN"),
        ] {
            w.users.insert(id.into(),serde_json::from_value(json!({"id":id,"name":name,"profile":{"display_name":name,"image_48":if id=="U1"{"https://fixture.invalid/poster.png"}else{""}}})).unwrap());
        }
        let asset = s
            .media
            .register_avatar("U1", "https://fixture.invalid/poster.png");
        s.media.insert(
            asset.clone(),
            "image/png",
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/icons/icon-512.png"
            ))
            .as_slice(),
        );
        assert!(s.media.is_ready(&asset));
    }
    for update in initial_updates {
        if let RtUpdate::ReadBarrier {
            generation,
            barrier,
        } = update
        {
            crate::thread_feed::accept_read_barrier(
                &mut initial.write(),
                "T1",
                generation,
                barrier,
            );
        }
    }
    let original_users = users(initial);
    let reader = source(initial);
    assert!(
        reader
            .run_with(initial, Mode::Refresh, |_| async { Ok(packet()) })
            .await
            .is_some()
    );
    let shell = std::mem::replace(
        &mut *initial.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    let (mut dom, mut state, captured) = mount(shell);
    let (before, avatars) = current_article(&captured, "DOC ROOT");
    assert!(
        before.contains("U2")
            && before.contains("U3")
            && before.contains("UMISSING")
            && before.contains("Unknown author")
    );
    assert!(
        !before.contains("CACHED POSTER")
            && !before.contains("CACHED EDITOR")
            && !before.contains("CACHED AUTHOR")
            && !before.contains("CACHED UNKNOWN")
    );
    assert_eq!(
        avatars, 0,
        "a READY posting-user face is still the wrong document attribution"
    );
    assert_eq!(
        current_article(&captured, "PLAIN ROOT").1,
        1,
        "the ordinary ready-avatar path remains intact"
    );
    assert!(open(state, "10.0").is_none() && open(state, "11.0").is_none());

    let requests = Rc::new(RefCell::new(Vec::new()));
    let seen = requests.clone();
    members::load_all_with(reader,state,move|_,request|{
        let seen=seen.clone();
        async move {
            let super_platinum_core::slack::client::RequestBody::Json(body)=request.body else{panic!("strict member JSON")};
            let ids=body["updated_ids"].as_object().unwrap().keys().cloned().collect::<Vec<_>>();
            assert_eq!(ids,vec!["U1".to_owned(),"U2".to_owned(),"U3".to_owned()]);
            seen.borrow_mut().push(ids);
            Ok(json!({"results":[member("U1","Fresh Posting User"),member("U2","Fresh Editor"),member("U3","Fresh First Author")]}))
        }
    }).await;
    dom.render_immediate_to_vec();
    let (after, avatars) = current_article(&captured, "DOC ROOT");
    assert!(
        after.contains("Fresh Editor")
            && after.contains("Fresh First Author")
            && after.contains("UMISSING")
    );
    assert!(!after.contains("Fresh Posting User") && !after.contains("CACHED UNKNOWN"));
    assert_eq!(avatars, 0);
    let (system, system_avatars) = current_article(&captured, "UNQUALIFIED SYSTEM");
    assert!(
        system.contains("Unknown author") && !system.contains("Fresh Editor"),
        "unqualified Slackbot/system override cannot become a human editor label"
    );
    assert_eq!(system_avatars, 0);
    for ts in ["10.0", "11.0", "12.0", "13.0"] {
        assert!(
            button(
                captured.borrow().as_ref().unwrap(),
                &format!("thread-profile-C1-10.0-{ts}")
            )
            .is_none(),
            "passive document identity grants no editor OR raw-user opener"
        );
    }
    // The same genuinely read raw posting user remains actionable on its
    // ordinary original occurrence; the document refusal is not cache absence.
    click(
        &dom,
        button(
            captured.borrow().as_ref().unwrap(),
            "thread-profile-C1-20.0-20.0",
        )
        .unwrap(),
    );
    dom.render_immediate_to_vec();
    assert_eq!(state.peek().profile_user.as_deref(), Some("U1"));
    state.write().close_profile();
    dom.render_immediate_to_vec();
    assert!(
        current_article(&captured, "DOC ROOT")
            .0
            .contains("Fresh Editor")
    );

    let changed = json!({"type":"message","subtype":"message_changed","channel":"C1","message":{"type":"message","channel":"C1","team":"T1","thread_ts":"10.0","ts":"10.0","document_comment":{"most_recent_editor":"U3","authors":["U2"]}}});
    for update in realtime::protected_updates(&connection, 12, &changed.to_string()) {
        match update {
            RtUpdate::Event { generation, event } => crate::realtime::apply_in_view_with_context(
                &mut state.write(),
                "T1",
                generation,
                *event,
                true,
                true,
            ),
            RtUpdate::ReadBarrier {
                generation,
                barrier,
            } => crate::thread_feed::accept_read_barrier(
                &mut state.write(),
                "T1",
                generation,
                barrier,
            ),
            _ => panic!("only original inert update/barrier"),
        }
    }
    dom.render_immediate_to_vec();
    let (edited, avatars) = current_article(&captured, "DOC ROOT");
    assert!(!edited.contains("Fresh Editor"));
    assert!(edited.contains("Fresh First Author") && edited.contains("UMISSING"));
    assert_eq!(avatars, 0);
    assert!(open(state, "10.0").is_none());
    assert_eq!(users(state), original_users);
    assert_eq!(
        requests.borrow().len(),
        1,
        "passive editor changes never acquire members or dispatch profile IO"
    );

    for alias in [
        "thread-feed-document-authors",
        "thread-feed-document-authors-narrow",
    ] {
        let shell = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
        let (_dom, _state, captured) = mount(shell);
        let body = text(captured.borrow().as_ref().unwrap());
        assert!(
            body.contains("UEDITOR") && body.contains("UAUTHOR") && body.contains("UUNRESOLVED")
        );
        assert!(!body.contains("View UEDITOR profile"));
    }
}
