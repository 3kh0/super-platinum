use super::*;
use serde_json::json;
fn setup() -> Workspace {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::message_buttons::conversation_fixture::install(&mut shell);
    shell.core.workspaces.remove("T1").unwrap()
}
#[test]
fn conversation_candidates_preserve_kind_and_external_filter_boundaries() {
    let w = setup();
    let mut f = Filter::default();
    let ids = |f: &Filter| {
        candidates(
            &w,
            f,
            w.users.values().map(|u| (u, false)),
            w.channels.values().map(|c| (c, false)),
            "",
        )
        .into_iter()
        .map(|c| c.id)
        .collect::<Vec<_>>()
    };
    let all = ids(&f);
    for id in [
        "C00000001",
        "G00000002",
        "G00000003",
        "C00000004",
        "U00000004",
        "USLACKBOT",
    ] {
        assert!(all.iter().any(|s| s == id), "{id}");
    }
    assert!(!all.iter().any(|s| s == "C00000005" || s == "D00000006"));
    f.spec.exclude_bot_users = true;
    f.spec.exclude_external_shared_channels = true;
    let filtered = ids(&f);
    assert!(filtered.iter().any(|s| s == "U00000004"));
    for id in ["C00000004", "U00000005", "USLACKBOT", "USLACKSECURITY"] {
        assert!(!filtered.iter().any(|s| s == id), "{id}");
    }
    f.spec.include = Some(vec!["mpim".into()]);
    assert_eq!(ids(&f), vec!["G00000003"]);
}
#[test]
fn team_filter_uses_derived_external_locality_and_source_precedence() {
    let w = setup();
    let mut f = Filter::default();
    f.spec.only_from_team_id = Some("TOTHER".into());
    f.spec.exclude_bot_users = true;
    let mut external: User = serde_json::from_value(
        json!({"id":"U00000020","team_id":"TOTHER","deleted":true,"is_bot":true}),
    )
    .unwrap();
    assert!(member_eligible(&w, &f, &external)); // captured early branch, before deleted/bot predicates
    external
        .extra
        .insert("enterprise_user".into(), json!({"teams":["T1"]}));
    assert!(!member_eligible(&w, &f, &external));
    external
        .extra
        .insert("enterprise_user".into(), json!({"enterprise_id":"ELOCAL"}));
    f.enterprise_id = Some("ELOCAL".into());
    assert!(!member_eligible(&w, &f, &external));
    assert!(member_eligible(&w, &f, &w.users[&w.self_user_id]));
    let mut channel: Channel = serde_json::from_value(
        json!({"id":"C00000020","is_channel":true,"name":"Other","team_id":"TOTHER"}),
    )
    .unwrap();
    assert!(!channel_eligible(&f, &channel));
    channel
        .extra
        .insert("connected_team_ids".into(), json!(["TOTHER"]));
    assert!(channel_eligible(&f, &channel));
}
#[test]
fn remote_fuzzy_matches_are_not_rejected_by_local_substring_and_bound_is_shared() {
    let w = setup();
    let f = Filter::default();
    let c: Channel =
        serde_json::from_value(json!({"id":"C00000030","name":"engineering","is_channel":true}))
            .unwrap();
    assert_eq!(
        candidates(
            &w,
            &f,
            std::iter::empty(),
            [(&c, true)].into_iter(),
            "enginer"
        )
        .len(),
        1
    );
    assert!(
        candidates(
            &w,
            &f,
            std::iter::empty(),
            [(&c, false)].into_iter(),
            "enginer"
        )
        .is_empty()
    );
    let users: Vec<User> = (100..150)
        .map(|n| {
            serde_json::from_value(json!({"id":format!("U{n:08}"),"name":format!("User {n}")}))
                .unwrap()
        })
        .collect();
    assert_eq!(
        candidates(
            &w,
            &f,
            users.iter().map(|u| (u, false)),
            [(&c, false)].into_iter(),
            ""
        )
        .len(),
        24
    );
}

#[test]
fn newer_remote_metadata_can_restore_choice_but_later_cache_change_blocks_it() {
    let mut w = setup();
    let f = Filter::default();
    let mut old: Channel = serde_json::from_value(
        json!({"id":"C00000050","name":"old","is_channel":true,"is_archived":true,"updated":10}),
    )
    .unwrap();
    w.channels.insert(old.id.clone(), old.clone());
    let mut remote = old.clone();
    remote.is_archived = false;
    remote.updated = Some(11);
    let found = candidates(
        &w,
        &f,
        std::iter::empty(),
        [(&remote, true)].into_iter(),
        "fuzzy",
    );
    assert_eq!(found.len(), 1);
    assert!(choice_current(&w, &f, &found[0]));
    old.updated = Some(12);
    w.channels.insert(old.id.clone(), old.clone());
    assert!(!choice_current(&w, &f, &found[0]));
    assert!(
        candidates(
            &w,
            &f,
            std::iter::empty(),
            [(&remote, true)].into_iter(),
            "fuzzy"
        )
        .is_empty()
    );
    remote.updated = None;
    old.updated = None;
    w.channels.insert(old.id.clone(), old);
    assert!(
        candidates(
            &w,
            &f,
            std::iter::empty(),
            [(&remote, true)].into_iter(),
            "fuzzy"
        )
        .is_empty()
    );
}

#[test]
fn nonempty_filter_without_include_inherits_generic_channel_defaults() {
    let w = setup();
    let mut f = Filter::default();
    assert!(channel_eligible(&f, &w.channels["G00000003"]));
    assert!(!channel_eligible(&f, &w.channels["C00000005"]));
    f.default_channels = true;
    assert!(!channel_eligible(&f, &w.channels["G00000003"]));
    assert!(channel_eligible(&f, &w.channels["C00000005"]));
}

#[test]
fn newer_remote_ineligible_metadata_suppresses_later_stale_cache_fallback() {
    let mut w = setup();
    let mut f = Filter::default();
    f.spec.exclude_external_shared_channels = true;
    let cached: Channel = serde_json::from_value(
        json!({"id":"C00000051","name":"local","is_channel":true,"updated":10}),
    )
    .unwrap();
    w.channels.insert(cached.id.clone(), cached.clone());
    for case in 0..3 {
        let mut remote = cached.clone();
        remote.updated = Some(11);
        match case {
            0 => remote.is_archived = true,
            1 => remote.is_ext_shared = true,
            _ => {
                remote.is_channel = false;
                remote.is_im = true;
            }
        }
        assert!(
            candidates(
                &w,
                &f,
                std::iter::empty(),
                [(&remote, true), (&cached, false)].into_iter(),
                ""
            )
            .is_empty(),
            "case {case}"
        );
    }
}
