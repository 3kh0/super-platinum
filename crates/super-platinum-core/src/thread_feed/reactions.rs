use super::*;
use serde_json::json;

fn item(channel: &str) -> ThreadViewItem {
    serde_json::from_value(json!({
        "root_msg":{"channel":channel,"ts":"100.000000","text":"Root","reactions":[{"name":"eyes","count":1,"users":["U1"]}]},
        "latest_replies":[{"channel":channel,"thread_ts":"100.000000","ts":"101.000000","text":"Reply","reactions":[{"name":"thumbsup","count":1,"users":["U1"]}]}],
        "unread_replies":[{"channel":channel,"thread_ts":"100.000000","ts":"101.000000","text":"Reply","reactions":[{"name":"thumbsup","count":1,"users":["U1"]}]}]
    })).unwrap()
}
fn event(ts: &str, user: &str, name: &str, added: bool) -> RtEvent {
    if added {
        RtEvent::ReactionAdded {
            channel: "C".into(),
            ts: ts.into(),
            user: user.into(),
            reaction: name.into(),
        }
    } else {
        RtEvent::ReactionRemoved {
            channel: "C".into(),
            ts: ts.into(),
            user: user.into(),
            reaction: name.into(),
        }
    }
}
fn counts(core: &CoreAppState) -> (u32, u32, u32) {
    let i = core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_msg.channel.as_deref() == Some("C"))
        .unwrap();
    (
        i.root_msg.reactions[0].count,
        i.latest_replies[0].reactions[0].count,
        i.unread_replies[0].reactions[0].count,
    )
}

#[test]
fn reactions_replay_held_pages_update_each_coherent_copy_and_keep_existing_guards() {
    let mut core = CoreAppState::new(Default::default());
    core.active_team = Some("T".into());
    core.threads_view.ensure_scope(core.account_epoch, "T");
    let mut own = item("C");
    let mut foreign = own.latest_replies[0].clone();
    foreign.channel = Some("OTHER".into());
    own.latest_replies.push(foreign);
    let page = ThreadsViewPage {
        threads: vec![own, item("PEER")],
        ..Default::default()
    };
    let first = core.threads_view.begin(false).unwrap();
    assert!(core.apply_thread_feed_page("T", &first, page.clone(), &HashMap::new()));
    let held = core.threads_view.begin(false).unwrap();
    for _ in 0..2 {
        core.threads_view
            .observe(&event("100.000000", "U2", "eyes", true));
        core.threads_view
            .observe(&event("101.000000", "U2", "thumbsup", true));
    }
    core.threads_view
        .observe(&event("101.000000", "", "thumbsup", true));
    core.threads_view
        .observe(&event("101.000000", "U2", "", true));
    assert_eq!(counts(&core), (2, 2, 2));
    assert!(core.apply_thread_feed_page("T", &held, page.clone(), &HashMap::new()));
    assert_eq!(
        counts(&core),
        (2, 2, 2),
        "older included HTTP data must not undo accepted socket reactions"
    );
    let own = core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_msg.channel.as_deref() == Some("C"))
        .unwrap();
    assert_eq!(
        own.latest_replies[1].reactions[0].count, 1,
        "foreign nested channel is not the event target"
    );
    let peer = core
        .threads_view
        .items
        .iter()
        .find(|i| i.root_msg.channel.as_deref() == Some("PEER"))
        .unwrap();
    assert_eq!(peer.root_msg.reactions[0].count, 1);
    assert_eq!(peer.latest_replies[0].reactions[0].count, 1);
    core.threads_view.observe(&RtEvent::MessageChanged {
        channel: "C".into(),
        message: Message {
            ts: Some("100.000000".into()),
            text: Some("Edited root".into()),
            ..Default::default()
        },
        original_update: None,
    });
    assert_eq!(
        counts(&core),
        (2, 2, 2),
        "an ordinary partial edit retains reactions"
    );
    for _ in 0..2 {
        core.threads_view
            .observe(&event("100.000000", "U2", "eyes", false));
        core.threads_view
            .observe(&event("101.000000", "U2", "thumbsup", false));
    }
    assert_eq!(
        counts(&core),
        (1, 1, 1),
        "known-member duplicate events retain the existing reducer semantics"
    );
    let retired = core.threads_view.begin(false).unwrap();
    core.account_epoch += 1;
    core.threads_view.ensure_scope(core.account_epoch, "T");
    assert!(!core.apply_thread_feed_page("T", &retired, page, &HashMap::new()));
    assert!(core.threads_view.items.is_empty());
}
