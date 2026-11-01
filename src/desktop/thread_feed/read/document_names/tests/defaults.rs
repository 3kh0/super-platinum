//! Ordinary numeric0 preferences resolve only from complete convergent data.
use super::*;

#[tokio::test(flavor = "current_thread")]
async fn thread_document_defaults_known_boolean_policy_resolves_cold_doc_only_names_without_client_flag_assumptions()
 {
    for real_names in [false, true] {
        let h = harness();
        let mut initial = h.state;
        let (connection, updates) = realtime::protected_connection(12);
        {
            let mut s = initial.write();
            s.main_view = MainView::Threads;
            s.overlay = None;
            s.core.threads_view = Default::default();
            s.core
                .session
                .as_mut()
                .unwrap()
                .unified_user_client_enabled
                .insert("T1".into(), false);
            let workspace = s.core.workspaces.get_mut("T1").unwrap();
            workspace.rt_generation = 12;
            workspace.rt =
                super_platinum_core::state::RealtimeStatus::Connected(connection.clone());
        }
        apply(&mut initial, updates);
        {
            let mut s = initial.write();
            let context = crate::scheduled::Context::capture(&s).unwrap();
            let boot = BootRead::begin(&mut s.core, &context.transport, &context.session).unwrap();
            assert!(boot.apply(&mut s.core,json!({"ok":true,"team":{"id":"T1","plan":"plus"},"self":{"id":context.session.user_id,"team_id":"T1"},"workspaces":[{"id":"T1"}],"read_only_channels":[],"thread_only_channels":[]}),false).unwrap());
        }
        let page:ThreadsViewPage = serde_json::from_value(json!({"threads":[{"root_msg":{"type":"message","channel":"C1","team":"T1","ts":"1719800000.000000","user":"UPOST","text":"Default policy document","reply_count":1,"document_comment":{"most_recent_editor":"UD000"}},"latest_replies":[{"type":"message","channel":"C1","team":"T1","ts":"1719800001.000000","thread_ts":"1719800000.000000","user":"UPOST","text":"Hidden author document","document_comment":{"authors":["UD001"]}}]}],"has_more":false})).unwrap();
        let reader = Source::capture(&initial.peek()).unwrap();
        assert!(
            reader
                .run_with(initial, Mode::Refresh, |_| async { Ok(page) })
                .await
                .is_some()
        );
        assert!(initial.peek().thread_feed_reads.profiles.is_none());
        let canonical = users(initial);
        let shell = std::mem::replace(
            &mut *initial.write(),
            ShellState::fixture(crate::media::MediaRegistry::default()),
        );
        let (mut dom, mut state, captured) = mount(shell);
        let io = Rc::new(profiles::test_io::Io::default());
        dom.in_scope(ScopeId::ROOT, || provide_context(io.clone()));
        dom.in_scope(ScopeId::ROOT, || schedule(state));
        assert!(io.reads.borrow().is_empty());
        let human = io.expect(ids(0, 2));
        let (release, wait) = tokio::sync::oneshot::channel();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let done = start_policy_with_team(
            &dom,
            state,
            json!(0),
            json!(real_names),
            Some(wait),
            calls.clone(),
        );
        settle(&mut dom, || calls.borrow().len() == 2).await;
        assert!(
            io.reads.borrow().is_empty(),
            "cold policy is not a default Boolean"
        );
        release.send(()).unwrap();
        settle(&mut dom, || done.get() && io.reads.borrow().len() == 1).await;
        assert!(body(&captured).contains("Name hidden"));
        human.send(Ok(members(0, 2))).unwrap();
        let expected = if real_names {
            "Real UD000"
        } else {
            "Display UD000"
        };
        settle(&mut dom, || body(&captured).contains(expected)).await;
        let doc = article_text(&captured, "Default policy document");
        assert!(doc.contains(expected) && doc.contains("Name hidden"));
        assert!(!doc.contains("Display UD001") && !doc.contains("Real UD001"));
        assert!(!body(&captured).contains(&format!("View {expected} profile")));
        assert_eq!(*io.reads.borrow(), vec![ids(0, 2)]);
        assert_eq!(users(state), canonical);
        assert!(state.peek().thread_feed_reads.profiles.is_none());
        // The actual admitted preference event retires the complete policy.
        // No missing prefs/flag/cache value substitutes for it.
        let changed = json!({"type":"pref_change","name":"display_real_names_override","value":0});
        apply(
            &mut state,
            realtime::protected_updates(&connection, 12, &changed.to_string()),
        );
        dom.render_immediate_to_vec();
        assert!(!body(&captured).contains(expected));
        dom.in_scope(ScopeId::ROOT, || schedule(state));
        dom.render_immediate_to_vec();
        assert_eq!(io.reads.borrow().len(), 1);
    }
}
