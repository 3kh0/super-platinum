use super::*;
use super_platinum_core::{
    drafts::{DraftKey, LocalDraft},
    slack::models::Channel,
};
fn setup() -> (ShellState, String, BTreeSet<String>) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let team = shell.core.workspaces.keys().next().unwrap().clone();
    let channel:Channel=serde_json::from_value(json!({"id":"C12345678","name":"alpha","is_channel":true,"last_read":"1.000001","unread_count":2})).unwrap();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .insert(channel.id.clone(), channel);
    (shell, team, BTreeSet::from(["C12345678".into()]))
}
#[test]
fn successful_complete_preferences_only_supply_empty_history_default() {
    assert_eq!(
        history(&json!({"ok":true,"prefs":{}}))
            .unwrap()
            .to_wire()
            .unwrap(),
        "{}"
    );
    for raw in [
        json!({"ok":false,"prefs":{}}),
        json!({"ok":true}),
        json!({"ok":true,"error":"bad","prefs":{}}),
        json!({"ok":true,"prefs":{"frecency":{}}}),
        json!({"ok":true,"prefs":{"frecency":"not json"}}),
    ] {
        assert!(history(&raw).is_err(), "{raw}");
    }
    let raw = json!({"ok":true,"prefs":{"frecency":"{}"}});
    assert_eq!(history(&raw).unwrap().to_wire().unwrap(), "{}");
}
#[test]
fn assignment_success_has_source_defaults_but_failure_is_not_empty_success() {
    let (facts, pass, badge) = experiments(&json!({"ok":true,"assignments":{}})).unwrap();
    assert_eq!(
        facts,
        json!({"autocomplete_affinity_score":null,"autocomplete_affinity_ml_model":null})
    );
    assert!(!pass && !badge);
    let (_,pass,badge)=experiments(&json!({"ok":true,"assignments":{"autocomplete_flannel_fuzzy_match_passthrough":{"group":"treatment"},"badge_all_unreads":{"group":"on"}}})).unwrap();
    assert!(pass && badge);
    for raw in [
        json!({"ok":false,"assignments":{}}),
        json!({"ok":true}),
        json!({"ok":true,"error":"failed","assignments":{}}),
        json!({"ok":true,"assignments":{"badge_all_unreads":{"group":false}}}),
    ] {
        assert!(experiments(&raw).is_err());
    }
}
#[test]
fn canonical_rows_not_typed_or_remote_facts_supply_candidates() {
    let row = json!({"id":"C12345678","name":"alpha","is_channel":true,"fromAnotherTeam":false});
    assert_eq!(
        admitted_rows(&[row.clone()], &[row.clone(), row.clone()]).unwrap(),
        BTreeSet::from(["C12345678".into()])
    );
    let mut changed = row.clone();
    changed["name"] = json!("wire renamed");
    assert!(admitted_rows(&[row.clone()], &[changed]).is_err());
    assert!(admitted_rows(&[], &[row.clone()]).is_err());
    let mut unsupported = row;
    unsupported["is_im"] = json!(true);
    assert!(admitted_rows(&[unsupported.clone()], &[unsupported]).is_err());
}
#[test]
fn actual_messages_override_native_channel_read_and_unread_fields() {
    let (mut shell, team, ids) = setup();
    let before = native(&shell, &team, &ids).unwrap();
    let before = &before["metadata"]["C12345678"];
    assert_eq!(before["lastRead"], "1.000001");
    assert_eq!(before["unread"], true);
    assert!(before["latest"].is_null());
    let ws = shell.core.workspaces.get_mut(&team).unwrap();
    let messages = ws.messages.entry("C12345678".into()).or_default();
    messages.latest = Some("9.000001".into());
    messages.last_read = Some("8.000001".into());
    messages.unread_count = 0;
    ws.priority_scores.insert("C12345678".into(), 0.75);
    let after = native(&shell, &team, &ids).unwrap();
    let after = &after["metadata"]["C12345678"];
    assert_eq!(after["latest"], "9.000001");
    assert_eq!(after["lastRead"], "8.000001");
    assert_eq!(after["unread"], false);
    assert_eq!(after["priority"], 0.75);
    assert!(native(&shell, "foreign", &ids).is_err());
    assert!(native(&shell, &team, &BTreeSet::from(["C99999999".into()])).is_err());
}
#[test]
fn actual_lazy_presence_may_refresh_but_local_draft_and_other_changes_retire_seed() {
    let (mut shell, team, ids) = setup();
    let before = native(&shell, &team, &ids).unwrap();
    let revision = draft_revision(&shell, &team).unwrap();
    let ws = shell.core.workspaces.get_mut(&team).unwrap();
    ws.draft_presence.observe(&json!({"id":"DR1","client_msg_id":"client1","last_updated_ts":"1","destinations":[{"channel_id":"C12345678"}]})).unwrap();
    assert_ne!(draft_revision(&shell, &team), Some(revision));
    let remote = native(&shell, &team, &ids).unwrap();
    assert_eq!(remote["metadata"]["C12345678"]["draft"], true);
    assert_eq!(without_drafts(&before), without_drafts(&remote));
    shell.core.drafts.items.insert(
        DraftKey {
            team: team.clone(),
            channel: "C12345678".into(),
            thread: None,
        },
        LocalDraft::default(),
    );
    let local = native(&shell, &team, &ids).unwrap();
    assert_ne!(without_drafts(&remote), without_drafts(&local));
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .draft_presence
        .retire_server_id("DR1");
    assert_eq!(
        native(&shell, &team, &ids).unwrap()["metadata"]["C12345678"]["draft"],
        true,
        "server purge cannot clear local draft"
    );
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut("C12345678")
        .unwrap()
        .is_starred = true;
    assert_ne!(
        without_drafts(&local),
        without_drafts(&native(&shell, &team, &ids).unwrap())
    );
}
#[test]
fn held_draft_page_cannot_overwrite_a_newer_socket_presence_observation() {
    use super_platinum_core::drafts::presence::Read;
    let (mut shell, team, _) = setup();
    let ws = shell.core.workspaces.get_mut(&team).unwrap();
    let mut read = Read::begin(&mut ws.draft_presence);
    ws.draft_presence.observe(&json!({"id":"DR1","client_msg_id":"client1","last_updated_ts":"3","destinations":[{"channel_id":"C12345678"}]})).unwrap();
    assert!(!read.current(&ws.draft_presence));
    assert!(
        read.active(
            &mut ws.draft_presence,
            &json!({"ok":true,"drafts":[],"has_more":false})
        )
        .is_err()
    );
    assert!(ws.draft_presence.has_channel("C12345678"));
}

#[test]
fn cached_preferences_require_original_account_cookie_credentials_and_revision() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::app_home::channel_fetches::fixture::install(&mut shell);
    let context = Arc::new(crate::scheduled::Context::capture(&shell).unwrap());
    let team = context.session.team_id.clone();
    let prefs = Preferences {
        context,
        account: shell.core.active_account.clone().unwrap(),
        cookie: shell.core.session.as_ref().unwrap().d_cookie.clone(),
        revision: autocomplete::preferences_revision(&shell),
        history: History::default(),
        experiments: json!({}),
        assignments: json!({"ok":true,"assignments":{}}),
        user_prefs: json!({"ok":true,"prefs":{}}),
        passthrough: false,
        badge_all_unreads: false,
    };
    assert!(prefs.current(&shell));
    let account = shell.core.active_account.replace("replacement".into());
    assert!(!prefs.current(&shell));
    shell.core.active_account = account;
    let cookie = std::mem::replace(
        &mut shell.core.session.as_mut().unwrap().d_cookie,
        "replacement-cookie".into(),
    );
    assert!(!prefs.current(&shell));
    shell.core.session.as_mut().unwrap().d_cookie = cookie;
    let token = std::mem::replace(
        &mut shell
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut(&team)
            .unwrap()
            .token,
        "replacement-token".into(),
    );
    assert!(!prefs.current(&shell));
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .token = token;
    assert!(prefs.current(&shell));
    shell.core.account_epoch = shell.core.account_epoch.wrapping_add(1);
    assert!(!prefs.current(&shell));
}

#[test]
fn list_locality_expansion_keeps_profile_source_admission_unchanged() {
    type List = crate::slack_lists::comments::completion::Lease;
    let foreign =
        json!({"id":"C12345678","name":"foreign","is_channel":true,"fromAnotherTeam":true});
    assert!(admitted_rows(&[foreign.clone()], &[foreign.clone()]).is_err());
    assert!(admitted_rows_for::<List>(&[foreign.clone()], &[foreign.clone()]).is_ok());
    let mut guessed = foreign.clone();
    guessed.as_object_mut().unwrap().remove("fromAnotherTeam");
    assert!(admitted_rows_for::<List>(&[guessed.clone()], &[guessed]).is_err());
    let mut modified = foreign.clone();
    modified["name"] = json!("different");
    assert!(admitted_rows_for::<List>(&[foreign], &[modified]).is_err());
}
#[test]
fn friendly_assignment_requires_complete_success_and_preserves_absent_false() {
    assert_eq!(
        assignment(
            &json!({"ok":true,"assignments":{}}),
            "friendly_channel_names"
        )
        .unwrap(),
        None
    );
    assert_eq!(
        assignment(
            &json!({"ok":true,"assignments":{"friendly_channel_names":{"group":"on"}}}),
            "friendly_channel_names"
        )
        .unwrap(),
        Some("on")
    );
    for raw in [
        json!({}),
        json!({"ok":false,"assignments":{}}),
        json!({"ok":true,"assignments":{"friendly_channel_names":{"group":true}}}),
    ] {
        assert!(assignment(&raw, "friendly_channel_names").is_err());
    }
}
