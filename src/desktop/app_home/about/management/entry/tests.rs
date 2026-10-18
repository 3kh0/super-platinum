use super::*;
use crate::message_buttons::source::Bag;

fn setup() -> (ShellState, u64) {
    let mut s = super::super::tests::shell();
    let editor = super::super::tests::editor(&mut s);
    fixture::install(&mut s);
    let w = &s.core.workspaces["TWORKFLOW"];
    let raw = w.messages["C2"]
        .messages
        .iter()
        .find(|m| m.ts.as_deref() == Some(fixture::TS))
        .unwrap();
    let row = crate::message_vm::message_vm(w, raw, &s.media);
    let source = Source::capture(&s, &row, "C2", Bag::Timeline).unwrap();
    let owner = Owner::message(&s, source).unwrap();
    let visit = next();
    let doc = Document {
        id: next(),
        account: s.core.active_account.clone(),
        team: owner.ctx.session.team_id.clone(),
        user: owner.ctx.session.user_id.clone(),
        editor,
        title: "Unsaved workflow title".into(),
        description: "Exact draft\n中🙂".into(),
        revision: next(),
        observed: None,
        pending: None,
        notice: None,
        activity: activity::State::default(),
    };
    s.app_home.about.management.surface = Some(Surface {
        id: visit,
        about: None,
        entry: Some(Target {
            workflow: "WF_REVIEW".into(),
            execution: Some("E_REVIEW".into()),
            auto_pending: false,
            loaded: false,
        }),
        owner,
        account: s.core.active_account.clone(),
        query: ListQuery::default(),
        list: List::default(),
        document: Some(doc.id),
        read: None,
        policy: None,
        notice: None,
        fixture: true,
    });
    s.app_home.about.management.documents.push(doc);
    let policy = policy::fixture_snapshot(&mut s);
    s.app_home.about.management.surface.as_mut().unwrap().policy = Some(policy);
    assert!(showing(&s));
    (s, visit)
}

#[test]
fn direct_message_entry_needs_no_about_or_bot_dm_and_preserves_conversation() {
    let (s, visit) = setup();
    assert!(s.app_home.about.surface.is_none());
    assert!(s.app_home.surface.is_none());
    assert_eq!(s.core.active_channel.as_deref(), Some("C2"));
    assert!(s.core.workspaces["TWORKFLOW"].channels["C2"].is_channel);
    assert!(visible(&s, visit));
    assert_eq!(snapshot(&s)["workflow"], "WF_REVIEW");
    assert_eq!(snapshot(&s)["execution"], "E_REVIEW");
}

#[test]
fn original_message_owner_refuses_account_session_transport_socket_and_source_changes() {
    for change in 0..7 {
        let (mut s, visit) = setup();
        match change {
            0 => s.core.account_epoch += 1,
            1 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .d_cookie
                .push_str("-replacement"),
            2 => {
                s.core.transport = Some(Arc::new(
                    super_platinum_core::slack::Transport::disconnected_fixture("fixture"),
                ))
            }
            3 => {
                s.core
                    .workspaces
                    .get_mut("TWORKFLOW")
                    .unwrap()
                    .rt_generation += 1
            }
            4 => s.channel_generation += 1,
            5 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("TWORKFLOW")
                .unwrap()
                .token
                .push_str("-replacement"),
            _ => s
                .core
                .workspaces
                .get_mut("TWORKFLOW")
                .unwrap()
                .messages
                .get_mut("C2")
                .unwrap()
                .messages
                .clear(),
        }
        assert!(!showing(&s), "retirement {change}");
        assert!(document(&s, visit).is_none());
        loaded(&mut s, visit);
        assert_eq!(snapshot(&s)["autoPending"], false);
    }
}

#[test]
fn successful_load_arms_only_once_and_keeps_original_metadata_draft() {
    let (mut s, visit) = setup();
    let original = document(&s, visit).unwrap().clone();
    loaded(&mut s, visit);
    assert_eq!(snapshot(&s)["autoPending"], true);
    assert!(
        s.app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .entry
            .as_ref()
            .unwrap()
            .loaded
    );
    // A duplicate completion after Activity becomes active must not reset it.
    s.app_home
        .about
        .management
        .surface
        .as_mut()
        .unwrap()
        .entry
        .as_mut()
        .unwrap()
        .auto_pending = false;
    document_mut(&mut s, visit).unwrap().activity.active = true;
    loaded(&mut s, visit);
    assert_eq!(snapshot(&s)["autoPending"], false);
    let actual = document(&s, visit).unwrap();
    assert!(actual.activity.active);
    assert_eq!(actual.id, original.id);
    assert_eq!(actual.revision, original.revision);
    assert_eq!(actual.title, original.title);
    assert_eq!(actual.description, original.description);
}

#[test]
fn wrong_workflow_or_visit_cannot_arm_or_close_successor() {
    let (mut s, visit) = setup();
    loaded(&mut s, visit + 1);
    close(&mut s, visit + 1);
    assert!(showing(&s));
    assert_eq!(snapshot(&s)["autoPending"], false);
    s.app_home
        .about
        .management
        .surface
        .as_mut()
        .unwrap()
        .entry
        .as_mut()
        .unwrap()
        .workflow = "OTHER".into();
    loaded(&mut s, visit);
    assert_eq!(snapshot(&s)["autoPending"], false);
}

#[test]
fn closing_direct_visit_retains_exact_document_and_conversation_draft() {
    let (mut s, visit) = setup();
    s.core.composer.text = "Conversation draft 中🙂".into();
    let original = document(&s, visit).unwrap().clone();
    let generation = s.channel_generation;
    close(&mut s, visit);
    assert!(!showing(&s));
    let retained = s
        .app_home
        .about
        .management
        .documents
        .iter()
        .find(|d| d.id == original.id)
        .unwrap();
    assert_eq!(retained.title, original.title);
    assert_eq!(retained.description, original.description);
    assert_eq!(retained.revision, original.revision);
    assert_eq!(s.core.composer.text, "Conversation draft 中🙂");
    assert_eq!(s.core.active_channel.as_deref(), Some("C2"));
    assert_eq!(s.channel_generation, generation);
}

/// Build actual source bags; the direct visit/document machinery stays shared.
fn source_surface(case: usize) -> (ShellState, u64) {
    let (mut s, visit) = setup();
    let mut raw = s.core.workspaces["TWORKFLOW"].messages["C2"].messages[0].clone();
    let root = "1781524800.000000";
    raw.thread_ts = Some(root.into());
    let bag = if case == 2 {
        s.channel_tab = crate::model::ChannelTab::Pins;
        let item = super_platinum_core::pins::PinnedItem {
            kind: "message".into(),
            channel: Some("C2".into()),
            message: Some(raw.clone()),
            ..Default::default()
        };
        let key = item.key().unwrap();
        s.core.pins.ensure_scope(s.core.account_epoch, "TWORKFLOW");
        let generation = s.core.pins.begin("C2");
        assert!(s.core.pins.apply(
            generation,
            super_platinum_core::pins::PinsPage { items: vec![item] }
        ));
        Bag::Pins { item_key: key }
    } else {
        s.thread_root = Some(root.into());
        s.core
            .threads
            .entry(("TWORKFLOW".into(), "C2".into(), root.into()))
            .or_default()
            .upsert(raw.clone());
        if case == 1 {
            s.main_view = crate::model::MainView::Activity;
            s.surfaces.insert(
                s.main_view,
                crate::model::SurfaceTarget {
                    channel: "C2".into(),
                    thread_root: Some(root.into()),
                },
            );
        }
        Bag::Thread { root: root.into() }
    };
    let row = crate::message_vm::message_vm(&s.core.workspaces["TWORKFLOW"], &raw, &s.media);
    let source = Source::capture(&s, &row, "C2", bag).unwrap();
    let owner = Owner::message(&s, source).unwrap();
    s.app_home.about.management.surface.as_mut().unwrap().owner = owner;
    assert!(showing(&s));
    (s, visit)
}

#[test]
fn thread_and_pin_entry_load_close_preserve_source_surface_and_both_drafts() {
    for case in 0..3 {
        let (mut s, visit) = source_surface(case);
        s.core.composer.text = "Channel draft \u{4e2d}\u{1f642}".into();
        s.core.thread_composer.text = "Thread draft e\u{301}".into();
        let original = document(&s, visit).unwrap().clone();
        let before = (
            s.main_view,
            s.channel_tab,
            s.thread_root.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.pins.generation,
            s.surfaces.clone(),
        );
        if case == 1 {
            assert!(s.detail_open());
            assert!(
                !s.conversation_visible(),
                "focused Activity thread must not mount a channel composer"
            );
        }
        loaded(&mut s, visit);
        assert_eq!(snapshot(&s)["autoPending"], true);
        close(&mut s, visit);
        assert!(!showing(&s));
        let after = (
            s.main_view,
            s.channel_tab,
            s.thread_root.clone(),
            s.channel_generation,
            s.thread_generation,
            s.core.pins.generation,
            s.surfaces.clone(),
        );
        assert_eq!(after, before, "surface {case}");
        assert_eq!(s.core.composer.text, "Channel draft \u{4e2d}\u{1f642}");
        assert_eq!(s.core.thread_composer.text, "Thread draft e\u{301}");
        let retained = s
            .app_home
            .about
            .management
            .documents
            .iter()
            .find(|d| d.id == original.id)
            .unwrap();
        assert_eq!(retained.title, original.title);
        assert_eq!(retained.description, original.description);
        assert_eq!(retained.revision, original.revision);
        if case == 2 {
            assert_eq!(s.core.pins.items.len(), 1);
        } else {
            assert!(s.core.threads.contains_key(&(
                "TWORKFLOW".into(),
                "C2".into(),
                s.thread_root.clone().unwrap()
            )));
        }
    }
}

#[test]
fn thread_and_pin_retirement_prevents_direct_owner_and_late_load_admission() {
    for case in 0..3 {
        let (mut s, visit) = source_surface(case);
        let owner = s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .owner
            .clone();
        assert!(owner.current(&s));
        match case {
            0 => {
                let root = s.thread_root.clone().unwrap();
                s.begin_thread(root, None);
            }
            1 => s.thread_root = Some("replacement-root".into()),
            _ => {
                s.core.pins.begin("C2");
            }
        }
        assert!(!owner.current(&s), "retired source {case}");
        assert!(!showing(&s));
        loaded(&mut s, visit);
        assert_eq!(snapshot(&s)["autoPending"], false);
        assert!(
            !s.app_home
                .about
                .management
                .surface
                .as_ref()
                .unwrap()
                .entry
                .as_ref()
                .unwrap()
                .loaded
        );
    }
}
