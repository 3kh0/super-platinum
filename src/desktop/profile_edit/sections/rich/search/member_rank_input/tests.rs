use super::*;
use super_platinum_core::{
    drafts::{DraftKey, LocalDraft},
    slack::models::{Channel, User},
};
fn setup() -> (ShellState, String, BTreeSet<String>) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    let team = s.core.workspaces.keys().next().unwrap().clone();
    let ws = s.core.workspaces.get_mut(&team).unwrap();
    let user: User = serde_json::from_value(
        json!({"id":"U12345678","name":"native","profile":{"display_name":"Native"}}),
    )
    .unwrap();
    ws.users.insert(user.id.clone(), user);
    (s, team, BTreeSet::from(["U12345678".into()]))
}
fn add_dm(s: &mut ShellState, team: &str, id: &str) {
    let channel: Channel = serde_json::from_value(
        json!({"id":id,"is_im":true,"user":"U12345678","last_read":"1.000001","unread_count":2}),
    )
    .unwrap();
    s.core
        .workspaces
        .get_mut(team)
        .unwrap()
        .channels
        .insert(id.into(), channel);
}
#[test]
fn raw_source_admission_refuses_derived_wire_replacement_or_duplicate() {
    let row = json!({"id":"U12345678","profile":{"display_name":"Original"},"unknown":7});
    assert_eq!(
        admitted(&[row.clone()], &[row.clone()]).unwrap()["U12345678"],
        row
    );
    let mut changed = row.clone();
    changed["_display_name_normalized_lc"] = json!("original");
    assert!(admitted(&[row.clone()], &[changed]).is_err());
    assert!(admitted(&[row.clone()], &[row.clone(), row.clone()]).is_err());
    assert!(admitted(&[], &[row]).is_err());
}
#[test]
fn ordinary_lowercase_is_explicit_and_injected_selector_facts_do_not_survive() {
    let row = json!({"id":"U12345678","profile":{"real_name":"REAL","display_name":"DISPLAY","email":"MAIL"},
        "unknown":7,"_display_name_normalized_lc":"injected","facts":{"self":true,"comparisonName":"wire"}});
    let lower = |s: &str| match s {
        "REAL" => Ok("real".into()),
        "DISPLAY" => Ok("display".into()),
        "MAIL" => Ok("mail".into()),
        _ => Err(rank::Error::MissingObservation("field".into())),
    };
    let derived = derive(&row, &lower).unwrap();
    assert_eq!(derived["_display_name_normalized_lc"], "display");
    assert_eq!(derived["profile"], row["profile"]);
    assert_eq!(derived["unknown"], 7);
    let selected = selector(&derived, "Observed comparison").unwrap();
    assert_eq!(selected["facts"]["comparisonName"], "Observed comparison");
    assert_eq!(selected["facts"]["self"], false);
    assert!(
        derive(&row, &|_| Err(rank::Error::MissingObservation(
            "field".into()
        )))
        .is_err()
    );
}
#[test]
fn native_dm_identity_has_no_guessed_channel_and_messages_override_channel_cache() {
    let (mut s, team, ids) = setup();
    let empty = native(&s, &team, &ids).unwrap();
    assert!(empty["ims"].as_object().unwrap().is_empty());
    assert!(empty["metadata"].as_object().unwrap().is_empty());
    add_dm(&mut s, &team, "D12345678");
    let before = native(&s, &team, &ids).unwrap();
    assert_eq!(before["ims"]["U12345678"]["id"], "D12345678");
    assert!(before["metadata"]["U12345678"].is_null());
    assert_eq!(before["metadata"]["D12345678"]["unread"], true);
    let ws = s.core.workspaces.get_mut(&team).unwrap();
    let messages = ws.messages.entry("D12345678".into()).or_default();
    messages.latest = Some("9.000001".into());
    messages.last_read = Some("8.000001".into());
    messages.unread_count = 0;
    ws.priority_scores.insert("D12345678".into(), 0.75);
    let after = native(&s, &team, &ids).unwrap();
    assert_eq!(after["metadata"]["D12345678"]["latest"], "9.000001");
    assert_eq!(after["metadata"]["D12345678"]["lastRead"], "8.000001");
    assert_eq!(after["metadata"]["D12345678"]["unread"], false);
    assert_eq!(after["metadata"]["D12345678"]["priority"], 0.75);
    assert_ne!(without_drafts(&before), without_drafts(&after));
    add_dm(&mut s, &team, "D87654321");
    assert!(native(&s, &team, &ids).is_err());
    assert!(native(&s, "foreign", &ids).is_err());
}
#[test]
fn actual_lazy_presence_is_only_admissible_draft_delta() {
    let (mut s, team, ids) = setup();
    add_dm(&mut s, &team, "D12345678");
    let before = native(&s, &team, &ids).unwrap();
    let rev = revision(&s, &team);
    s.core.workspaces.get_mut(&team).unwrap().draft_presence.observe(&json!({"id":"DR1","client_msg_id":"client1","last_updated_ts":"1","destinations":[{"channel_id":"D12345678"}]})).unwrap();
    let remote = native(&s, &team, &ids).unwrap();
    assert_ne!(rev, revision(&s, &team));
    assert_eq!(remote["metadata"]["D12345678"]["draft"], true);
    assert_eq!(without_drafts(&before), without_drafts(&remote));
    s.core.drafts.items.insert(
        DraftKey {
            team: team.clone(),
            channel: "D12345678".into(),
            thread: None,
        },
        LocalDraft::default(),
    );
    let local = native(&s, &team, &ids).unwrap();
    assert_ne!(without_drafts(&remote), without_drafts(&local));
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .draft_presence
        .retire_server_id("DR1");
    assert_eq!(
        native(&s, &team, &ids).unwrap()["metadata"]["D12345678"]["draft"],
        true
    );
}
#[test]
fn native_member_drift_cannot_be_misclassified_as_lazy_draft_change() {
    let (mut s, team, ids) = setup();
    let before = native(&s, &team, &ids).unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut("U12345678")
        .unwrap()
        .deleted = true;
    assert_ne!(
        without_drafts(&before),
        without_drafts(&native(&s, &team, &ids).unwrap())
    );
    assert_eq!(before["users"]["U12345678"]["deleted"], false);
}
