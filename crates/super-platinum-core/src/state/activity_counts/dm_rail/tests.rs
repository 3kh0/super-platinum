use super::*;
use crate::{
    config::WorkspaceSession,
    slack::models::{Channel, User},
    state::ChannelMessages,
};
use serde_json::json;

fn workspace(enterprise: bool) -> Workspace {
    Workspace::from_session(&WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: enterprise.then(|| "E1".into()),
        user_id: "U_SELF".into(),
        name: "Test".into(),
        url: "https://fixture.invalid".into(),
        token: String::new(),
    })
}
fn add(w: &mut Workspace, id: &str, user: &str, count: u32) {
    w.channels.insert(
        id.into(),
        Channel {
            id: id.into(),
            is_im: true,
            user: Some(user.into()),
            mention_count: Some(count),
            has_unreads: count > 0,
            ..Default::default()
        },
    );
    w.messages.insert(
        id.into(),
        ChannelMessages {
            mention_count: count,
            ..Default::default()
        },
    );
}

#[test]
fn rail_sums_highlights_not_conversations_or_activity_dm_count() {
    let mut w = workspace(false);
    for (i, count) in [39, 3, 1, 1, 1, 1].into_iter().enumerate() {
        add(&mut w, &format!("D{i}"), &format!("U{i}"), count);
    }
    w.apply_activity_counts(serde_json::from_value(json!({"dm":4})).unwrap());
    assert_eq!(w.dm_rail_badge_count(), 46);
    // Closed DMs and unknown members are not rejected by 0SNR.
    assert!(
        w.channels
            .values()
            .all(|c| { c.extra.get("is_open").and_then(serde_json::Value::as_bool) != Some(true) })
    );
    w.messages.get_mut("D0").unwrap().mention_count = 0;
    assert!(
        w.channels["D0"].has_unreads,
        "metadata deliberately remains stale"
    );
    assert_eq!(
        w.dm_rail_badge_count(),
        7,
        "newer bag read outranks stale metadata count"
    );
}

#[test]
fn app_system_and_muted_ims_are_excluded_but_self_deleted_users_are_not() {
    let mut w = workspace(false);
    for (i, user) in [
        "USEARCH",
        "USLACKSECURITY",
        "USLACKBOT",
        "USLACK",
        "UBOT",
        "UFLAG",
        "UMUTED",
    ]
    .into_iter()
    .enumerate()
    {
        add(&mut w, &format!("D{i}"), user, 50);
    }
    w.users.insert(
        "UBOT".into(),
        User {
            id: "UBOT".into(),
            is_bot: true,
            ..Default::default()
        },
    );
    w.users.insert(
        "UFLAG".into(),
        serde_json::from_value(json!({"id":"UFLAG","is_slackbot":true})).unwrap(),
    );
    w.sidebar.muted.insert("D6".into());
    add(&mut w, "D_SELF", "U_SELF", 2);
    add(&mut w, "D_DELETED", "UDELETED", 3);
    w.users.insert(
        "UDELETED".into(),
        User {
            id: "UDELETED".into(),
            deleted: true,
            ..Default::default()
        },
    );
    assert_eq!(w.dm_rail_badge_count(), 5);
}

#[test]
fn authoritative_counts_read_clears_omitted_mentions_in_bag_and_metadata() {
    let mut w = workspace(false);
    add(&mut w, "DREAD", "U1", 39);
    assert_eq!(w.dm_rail_badge_count(), 39);
    w.apply_counts(
        serde_json::from_value(json!({"ims":[{
            "id":"DREAD","is_im":true,"has_unreads":false
        }]}))
        .unwrap(),
    );
    assert_eq!(w.messages["DREAD"].mention_count, 0);
    assert_eq!(w.channels["DREAD"].mention_count, Some(0));
    assert_eq!(w.dm_rail_badge_count(), 0);
}

#[test]
fn authoritative_forced_unread_replaces_old_mentions_with_one_highlight() {
    let mut w = workspace(false);
    add(&mut w, "DFORCED", "U1", 39);
    w.apply_counts(
        serde_json::from_value(json!({"ims":[{
            "id":"DFORCED","is_im":true,"has_unreads":true
        }]}))
        .unwrap(),
    );
    assert_eq!(w.messages["DFORCED"].mention_count, 0);
    assert_eq!(w.channels["DFORCED"].mention_count, Some(0));
    assert_eq!(w.dm_rail_badge_count(), 1);
}

#[test]
fn forced_unread_is_one_not_all_message_count_and_mpim_eligibility_is_source_bound() {
    let mut w = workspace(false);
    add(&mut w, "DFORCED", "U1", 0);
    w.channels.get_mut("DFORCED").unwrap().has_unreads = true;
    w.messages.get_mut("DFORCED").unwrap().unread_count = 99;
    assert_eq!(w.dm_rail_badge_count(), 1);
    for (id, extra, archived) in [
        ("GGOOD", json!({"is_member":true}), false),
        ("GLEFT", json!({"is_member":false}), false),
        ("GMISSING", json!({}), false),
        ("GFILE", json!({"is_member":true,"is_file":true}), false),
        ("GARCHIVE", json!({"is_member":true}), true),
        (
            "GOTHER",
            json!({"is_member":true,"fromAnotherTeam":true}),
            false,
        ),
    ] {
        let mut extra = extra;
        extra["id"] = json!(id);
        let mut c: Channel = serde_json::from_value(extra).unwrap();
        c.id = id.into();
        c.is_mpim = true;
        c.is_archived = archived;
        c.mention_count = Some(4);
        w.channels.insert(id.into(), c);
    }
    assert_eq!(w.dm_rail_badge_count(), 17);
    let mut enterprise = workspace(true);
    enterprise.channels = w.channels.clone();
    enterprise.messages = w.messages.clone();
    assert_eq!(enterprise.dm_rail_badge_count(), 17);
    enterprise.sidebar.muted.insert("GGOOD".into());
    assert_eq!(enterprise.dm_rail_badge_count(), 13);
}
