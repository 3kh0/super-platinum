//! Ordinary-channel execution buttons backed by canonical synthetic messages.
use super::*;
use super_platinum_core::{slack::models::Message, state::ChannelMessages};

pub(crate) const TS: &str = "1781524800.000001";
pub(crate) const SUCCESSOR_ROOT: &str = "1781524750.000001";
pub(crate) const SUCCESSOR_TS: &str = "1781524850.000001";
pub(crate) const SUCCESSOR_OPEN: &str = "View successor workflow execution";
pub(crate) const ROOT: &str = "1781524700.000001";
pub(crate) const COMPOSER: &str = "Uncommitted conversation draft \u{4e2d}\u{1f642}";
pub(crate) const THREAD_COMPOSER: &str = "Uncommitted thread reply \u{4e2d}\u{1f642}";
pub(crate) const OPEN: &str = "View workflow execution";
pub(crate) const MISSING: &str = "View missing execution";
pub(crate) const MALFORMED: &str = "Unavailable execution action";
pub(crate) fn enabled(s: &ShellState) -> bool {
    activity::fixture::enabled(s)
        && matches!(
            activity::fixture::case().as_str(),
            "execution"
                | "execution-failure"
                | "execution-retirement"
                | "execution-thread"
                | "execution-pins"
                | "execution-focused"
                | "execution-thread-retirement"
                | "execution-pins-retirement"
                | "execution-thread-replacement"
                | "execution-pin-removal"
        )
}
pub(crate) fn install(s: &mut ShellState) {
    install_case(s, &activity::fixture::case());
}
fn install_case(s: &mut ShellState, scenario: &str) {
    let focused = scenario == "execution-focused";
    let thread = matches!(
        scenario,
        "execution-thread" | "execution-thread-retirement" | "execution-thread-replacement"
    ) || focused;
    let pins = matches!(
        scenario,
        "execution-pins" | "execution-pins-retirement" | "execution-pin-removal"
    );
    s.channel_tab = crate::state::ChannelTab::Messages;
    s.overlay = None;
    s.app_modal = None;
    s.toast = None;
    s.app_home.surface = None;
    s.app_home.about.surface = None;
    s.app_home.about.management.surface = None;
    let w = s
        .core
        .workspaces
        .get_mut("TWORKFLOW")
        .expect("Execution fixture workspace");
    let channel = w.channels.get_mut("C2").expect("Execution fixture channel");
    channel.is_im = false;
    channel.is_mpim = false;
    channel.is_channel = true;
    channel.user = None;
    let element = |label: &str, value: Value| {
        json!({"type":"button","action_id":"workflow_error_view_logs","value":value.to_string(),
        "text":{"type":"plain_text","text":label},"accessibility_label":label})
    };
    let raw = json!({"ts":TS,"channel":"C2","user":"UWORKFLOW","bot_id":"B_EXECUTION",
        "bot_profile":{"id":"B_EXECUTION","app_id":"A_HOME","team_id":"TWORKFLOW","name":"Workflow execution"},
        "text":"Review an execution without leaving this conversation.",
        "blocks":[{"type":"section","block_id":"execution-description","text":{"type":"plain_text","text":"Review an execution without leaving this conversation."}},
            {"type":"actions","block_id":"execution-open","elements":[element(OPEN,json!({"type":"wfb","wf_id":"WF_REVIEW","execution_id":"E_REVIEW"}))]},
            {"type":"actions","block_id":"execution-missing","elements":[element(MISSING,json!({"type":"wfb","wf_id":"WF_REVIEW","execution_id":"E_ABSENT"}))]},
            {"type":"actions","block_id":"execution-malformed","elements":[element(MALFORMED,json!({"type":"wfb","wf_id":"WF_REVIEW","execution_id":7}))]}]});
    let mut message: Message = serde_json::from_value(raw).expect("Execution action source");
    let parent: Message = serde_json::from_value(json!({"ts":ROOT,"channel":"C2",
        "user":"UWORKFLOW","text":"A workflow execution was shared in this thread.",
        "reply_count":1,"latest_reply":TS,"thread_ts":ROOT}))
    .expect("Execution thread parent");
    if thread || pins {
        message.thread_ts = Some(ROOT.into());
    }
    let successor = if matches!(
        scenario,
        "execution-thread-replacement" | "execution-pin-removal"
    ) {
        let mut raw = serde_json::to_value(&message).expect("Original synthetic action");
        raw["ts"] = json!(SUCCESSOR_TS);
        raw["thread_ts"] = json!(SUCCESSOR_ROOT);
        raw["text"] = json!("A different source retains this workflow draft.");
        raw["blocks"] = json!([{"type":"section","block_id":"successor-description","text":{"type":"plain_text","text":"A different source retains this workflow draft."}},
            {"type":"actions","block_id":"execution-open","elements":[element(SUCCESSOR_OPEN,json!({"type":"wfb","wf_id":"WF_REVIEW","execution_id":"E_REVIEW"}))]}]);
        Some(serde_json::from_value::<Message>(raw).expect("Successor action"))
    } else {
        None
    };
    let mut bag = ChannelMessages::default();
    bag.loaded = true;
    bag.upsert(if thread || pins {
        parent.clone()
    } else {
        message.clone()
    });
    w.messages.insert("C2".into(), bag);
    s.refresh_from_core();
    let w = &s.core.workspaces["TWORKFLOW"];
    let rows = w.messages["C2"]
        .messages
        .iter()
        .map(|message| crate::message_vm::message_vm(w, message, &s.media))
        .collect();
    s.install_fixture_messages(rows);
    if thread {
        let mut bag = ChannelMessages::default();
        bag.loaded = true;
        bag.upsert(parent);
        bag.upsert(message);
        s.core
            .threads
            .insert(("TWORKFLOW".into(), "C2".into(), ROOT.into()), bag);
        if let Some(successor) = successor {
            let mut bag = ChannelMessages::default();
            bag.loaded = true;
            bag.upsert(serde_json::from_value(json!({"ts":SUCCESSOR_ROOT,"channel":"C2","user":"UWORKFLOW",
                "text":"Successor thread with its own draft.","reply_count":1,"latest_reply":SUCCESSOR_TS,"thread_ts":SUCCESSOR_ROOT})).unwrap());
            bag.upsert(successor);
            s.core.threads.insert(
                ("TWORKFLOW".into(), "C2".into(), SUCCESSOR_ROOT.into()),
                bag,
            );
        }
        s.begin_thread(ROOT.into(), None);
        s.thread_loading = false;
        s.refresh_thread_from_core();
        if focused {
            s.main_view = crate::state::MainView::Activity;
            s.surfaces.insert(
                crate::state::MainView::Activity,
                crate::model::SurfaceTarget {
                    channel: "C2".into(),
                    thread_root: Some(ROOT.into()),
                },
            );
        }
    } else if pins {
        use super_platinum_core::pins::{PinnedItem, PinsPage};
        s.core.pins.ensure_scope(s.core.account_epoch, "TWORKFLOW");
        let generation = s.core.pins.begin("C2");
        let mut items = vec![PinnedItem {
            kind: "message".into(),
            channel: Some("C2".into()),
            message: Some(message),
            created: Some(1781524900),
            created_by: Some("UWORKFLOW".into()),
            ..Default::default()
        }];
        if let Some(message) = successor {
            items.push(PinnedItem {
                kind: "message".into(),
                channel: Some("C2".into()),
                message: Some(message),
                created: Some(1781524901),
                created_by: Some("UWORKFLOW".into()),
                ..Default::default()
            });
        }
        assert!(s.core.pins.apply(generation, PinsPage { items }));
        s.channel_tab = crate::state::ChannelTab::Pins;
    }
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let w = s.core.workspaces.get("TWORKFLOW");
    let channel = w.and_then(|w| w.channels.get("C2"));
    json!({"team":s.core.active_team,"channel":s.core.active_channel,"epoch":s.core.account_epoch,
        "channelGeneration":s.channel_generation,"mainView":format!("{:?}",s.main_view),
        "detailOpen":s.detail_open(),"conversationVisible":s.conversation_visible(),
        "selectedSurface":s.surfaces.get(&s.main_view).map(|target|json!({"channel":target.channel,"threadRoot":target.thread_root})),
        "ordinaryChannel":channel.is_some_and(|c|c.is_channel&&!c.is_im&&!c.is_mpim&&c.user.is_none()),
        "aboutPresent":s.app_home.about.surface.is_some(),"homePresent":s.app_home.surface.is_some(),
        "messagesTab":s.channel_tab==crate::state::ChannelTab::Messages,
        "surface":if s.channel_tab==crate::state::ChannelTab::Pins{"pins"}else if s.thread_root.is_some(){"thread"}else{"timeline"},
        "threadRoot":s.thread_root,"threadGeneration":s.thread_generation,"threadScope":s.thread_scope(),
        "threadComposerModel":s.core.thread_composer.text,
        "threadSource":s.core.threads.get(&("TWORKFLOW".into(),"C2".into(),ROOT.into())).map(|b|&b.messages),
        "pins":{"scopeCurrent":s.core.pins.matches_scope(s.core.account_epoch,"TWORKFLOW"),"channel":s.core.pins.channel,
            "generation":s.core.pins.generation,"loaded":s.core.pins.loaded,
            "items":s.core.pins.items.iter().map(|(key,item)|json!({"key":key,"kind":item.kind,"channel":item.channel,"message":item.message})).collect::<Vec<_>>()},
        "composerModel":s.core.composer.text,"appModal":s.app_modal.is_some(),
        "documents":s.app_home.about.management.documents.iter().map(|d|json!({"id":d.id,"title":d.title,"description":d.description,"dirty":d.dirty()})).collect::<Vec<_>>(),
        "source":w.and_then(|w|w.messages.get("C2")).map(|b|b.messages.iter().map(|m|json!({"ts":m.ts,"blocks":m.blocks})).collect::<Vec<_>>())})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_buttons::source::{Bag, Source};
    fn setup(scenario: &str) -> ShellState {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        super::super::super::fixture::install(&mut s, false, false);
        install_case(&mut s, scenario);
        s
    }
    #[test]
    fn thread_reply_resolves_only_its_real_bag_and_projects_normally() {
        let s = setup("execution-thread");
        let w = &s.core.workspaces["TWORKFLOW"];
        assert!(
            w.messages["C2"]
                .messages
                .iter()
                .all(|m| m.blocks.is_empty())
        );
        let bag = &s.core.threads[&("TWORKFLOW".into(), "C2".into(), ROOT.into())];
        let reply = bag
            .messages
            .iter()
            .find(|m| m.ts.as_deref() == Some(TS))
            .unwrap();
        assert_eq!(reply.thread_ts.as_deref(), Some(ROOT));
        let vm = crate::message_vm::message_vm(w, reply, &s.media);
        assert!(Source::capture(&s, &vm, "C2", Bag::Timeline).is_err());
        assert!(Source::capture(&s, &vm, "C2", Bag::Thread { root: ROOT.into() }).is_ok());
        assert!(s.thread_messages.iter().any(|m| m.ts == TS));
        assert!(!s.thread_loading);
    }
    #[test]
    fn pinned_reply_resolves_only_its_scoped_pin_without_opening_thread() {
        let s = setup("execution-pins");
        let key = format!("message:{TS}");
        let reply = s.core.pins.items[&key].message.as_ref().unwrap();
        assert_eq!(reply.thread_ts.as_deref(), Some(ROOT));
        assert!(s.core.pins.loaded && !s.core.pins.loading);
        assert!(s.thread_root.is_none());
        let w = &s.core.workspaces["TWORKFLOW"];
        assert!(
            w.messages["C2"]
                .messages
                .iter()
                .all(|m| m.blocks.is_empty())
        );
        let vm = crate::message_vm::message_vm(w, reply, &s.media);
        assert!(Source::capture(&s, &vm, "C2", Bag::Timeline).is_err());
        assert!(Source::capture(&s, &vm, "C2", Bag::Pins { item_key: key }).is_ok());
    }
    #[test]
    fn focused_activity_thread_has_real_surface_without_conversation_mount() {
        let s = setup("execution-focused");
        assert_eq!(s.main_view, crate::state::MainView::Activity);
        let selected = &s.surfaces[&crate::state::MainView::Activity];
        assert_eq!(selected.channel, "C2");
        assert_eq!(selected.thread_root.as_deref(), Some(ROOT));
        assert!(s.detail_open());
        assert!(!s.conversation_visible());
        assert_eq!(s.thread_root.as_deref(), Some(ROOT));
        assert!(!s.thread_loading);
        let w = &s.core.workspaces["TWORKFLOW"];
        assert!(
            w.messages["C2"]
                .messages
                .iter()
                .all(|m| m.blocks.is_empty())
        );
        let bag = &s.core.threads[&("TWORKFLOW".into(), "C2".into(), ROOT.into())];
        let reply = bag
            .messages
            .iter()
            .find(|m| m.ts.as_deref() == Some(TS))
            .unwrap();
        assert_eq!(reply.thread_ts.as_deref(), Some(ROOT));
        let vm = crate::message_vm::message_vm(w, reply, &s.media);
        assert!(Source::capture(&s, &vm, "C2", Bag::Thread { root: ROOT.into() }).is_ok());
        assert!(Source::capture(&s, &vm, "C2", Bag::Timeline).is_err());
        assert!(s.thread_messages.iter().any(|m| m.ts == TS));
    }
    #[test]
    fn replacement_fixture_keeps_old_bag_but_only_selected_successor_admits_action() {
        let mut s = setup("execution-thread-replacement");
        let old_key = ("TWORKFLOW".into(), "C2".into(), ROOT.into());
        let new_key = ("TWORKFLOW".into(), "C2".into(), SUCCESSOR_ROOT.into());
        let old = s.core.threads[&old_key]
            .messages
            .iter()
            .find(|m| m.ts.as_deref() == Some(TS))
            .unwrap()
            .clone();
        let new = s.core.threads[&new_key]
            .messages
            .iter()
            .find(|m| m.ts.as_deref() == Some(SUCCESSOR_TS))
            .unwrap()
            .clone();
        let vm = crate::message_vm::message_vm(&s.core.workspaces["TWORKFLOW"], &old, &s.media);
        let source = Source::capture(&s, &vm, "C2", Bag::Thread { root: ROOT.into() }).unwrap();
        let original = serde_json::to_value(&s.core.threads[&old_key].messages).unwrap();
        let generation = s.thread_generation;
        s.begin_thread(SUCCESSOR_ROOT.into(), None);
        s.thread_loading = false;
        s.refresh_thread_from_core();
        assert_eq!(s.thread_generation, generation + 1);
        assert_eq!(
            serde_json::to_value(&s.core.threads[&old_key].messages).unwrap(),
            original
        );
        assert!(source.client_message(&s).is_err());
        let vm = crate::message_vm::message_vm(&s.core.workspaces["TWORKFLOW"], &new, &s.media);
        assert!(
            Source::capture(
                &s,
                &vm,
                "C2",
                Bag::Thread {
                    root: SUCCESSOR_ROOT.into()
                }
            )
            .is_ok()
        );
        assert!(
            s.core.workspaces["TWORKFLOW"].messages["C2"]
                .messages
                .iter()
                .all(|m| m.blocks.is_empty())
        );
    }
    #[test]
    fn pin_removal_fixture_uses_real_event_and_preserves_independent_survivor() {
        let mut s = setup("execution-pin-removal");
        let key = format!("message:{TS}");
        let survivor = format!("message:{SUCCESSOR_TS}");
        let message = s.core.pins.items[&key].message.as_ref().unwrap();
        let vm = crate::message_vm::message_vm(&s.core.workspaces["TWORKFLOW"], message, &s.media);
        let source = Source::capture(
            &s,
            &vm,
            "C2",
            Bag::Pins {
                item_key: key.clone(),
            },
        )
        .unwrap();
        let saved = serde_json::to_value(s.core.pins.items[&survivor].message.as_ref()).unwrap();
        let generation = s.core.pins.generation;
        let raw=serde_json::from_value(json!({"type":"pin_removed","channel_id":"C2","item":{"type":"message","ts":TS},"event_ts":"1781524999.000001"})).unwrap();
        let update = super_platinum_core::pins::PinUpdate::from_event(&raw).unwrap();
        assert!(s.core.apply_pin_event("TWORKFLOW", &update));
        s.core.update_pin_copies("TWORKFLOW", "C2", TS, false);
        assert_eq!(s.core.pins.generation, generation + 1);
        assert!(!s.core.pins.items.contains_key(&key));
        assert_eq!(
            serde_json::to_value(s.core.pins.items[&survivor].message.as_ref()).unwrap(),
            saved
        );
        assert_eq!(s.core.pins.version("C2", &key), Some("1781524999.000001"));
        assert!(source.client_message(&s).is_err());
        let message = s.core.pins.items[&survivor].message.as_ref().unwrap();
        let vm = crate::message_vm::message_vm(&s.core.workspaces["TWORKFLOW"], message, &s.media);
        assert!(Source::capture(&s, &vm, "C2", Bag::Pins { item_key: survivor }).is_ok());
    }
}
