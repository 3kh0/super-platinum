use super::*;
use serde_json::json;

fn person(id: &str, name: &str, flag: Option<bool>) -> User {
    let mut raw = json!({"id":id,"name":name});
    if let Some(flag) = flag {
        raw["is_profile_only_user"] = json!(flag);
    }
    serde_json::from_value(raw).unwrap()
}

#[test]
fn cached_discovery_excludes_new_profile_only_people_but_keeps_selected_identity() {
    let (mut s, id) = super::super::tests::setup();
    let ctx = context(&s, id).unwrap();
    let self_id = ctx.session.user_id.clone();
    let ordinary = person("UCALORDINARY", "Directory Ordinary", None);
    let selected = person("UCALPROFILE", "Directory Profile", Some(true));
    let mut malformed = person("UCALMALFORMED", "Directory Malformed", None);
    malformed
        .extra
        .insert("is_profile_only_user".into(), json!(null));
    let workspace = s.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
    for user in [
        person(&self_id, "Directory Self", Some(false)),
        ordinary.clone(),
        selected.clone(),
        malformed,
    ] {
        workspace.users.insert(user.id.clone(), user);
    }
    let rows = load::share_choices(&s, &ctx, "Directory", "");
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        [ordinary.id.as_str(), self_id.as_str()]
    );
    for query in ["", "no matching name"] {
        let rows = load::share_choices(&s, &ctx, query, &selected.id);
        assert_eq!(rows[0].id, selected.id);
    }
    let c = active_mut(&mut s, id).unwrap();
    c.share_channel = selected.id.clone();
    c.share_selected = Some(Destination::User(selected));
    assert_eq!(resolve(&s, id).as_deref(), Some(""));
}

#[test]
fn actual_publication_filters_new_raw_and_cached_profiles_without_erasing_retained_selection() {
    for case in 0..4 {
        let (mut s, id) = super::super::tests::setup();
        let ctx = context(&s, id).unwrap();
        let target_id = "UCALDISCOVERY";
        let selected = case >= 2;
        let mut target = person(target_id, "Directory Remote", Some(case == 0 || case == 2));
        target.deleted = case == 3;
        let cached = person(target_id, "Directory Cached", Some(case == 1 || case == 2));
        s.core
            .workspaces
            .get_mut(&ctx.session.team_id)
            .unwrap()
            .users
            .insert(target_id.into(), cached);
        let before_users =
            serde_json::to_value(&s.core.workspaces[&ctx.session.team_id].users).unwrap();
        let c = active_mut(&mut s, id).unwrap();
        c.share_generation = 17;
        c.share_loading = true;
        c.share_query = "Directory".into();
        c.title = "Retained calendar draft".into();
        c.notice = Some("Retained unrelated preference warning".into());
        if selected {
            c.share_channel = target_id.into();
            c.share_selected = Some(Destination::User(target.clone()));
        }
        let selected_before = c.share_channel.clone();
        let resolve_before = resolve(&s, id);
        let sibling = Destination::User(person("UCALSIBLING", "Directory Sibling", None));
        publish(
            &mut s,
            id,
            17,
            &ctx,
            "Directory",
            vec![Destination::User(target), sibling],
            false,
        );
        let c = active(&s, id).unwrap();
        assert!(!c.share_loading, "case={case}");
        assert_eq!(c.share_channel, selected_before);
        assert_eq!(c.title, "Retained calendar draft");
        assert_eq!(
            c.notice.as_deref(),
            Some("Retained unrelated preference warning")
        );
        assert_eq!(resolve(&s, id), resolve_before);
        assert_eq!(
            c.share_remote
                .iter()
                .map(Destination::id)
                .collect::<Vec<_>>(),
            if case == 2 {
                vec![target_id, "UCALSIBLING"]
            } else {
                vec!["UCALSIBLING"]
            },
            "case={case}"
        );
        assert!(c.share_choices.iter().any(|row| row.id == "UCALSIBLING"));
        assert_eq!(
            c.share_choices.iter().any(|row| row.id == target_id),
            case != 1,
            "A separately eligible cached identity can remain even when its remote row is excluded; case={case}"
        );
        assert_eq!(
            serde_json::to_value(&s.core.workspaces[&ctx.session.team_id].users).unwrap(),
            before_users
        );
    }

    // Retained selection is independent of both cached choices and new results.
    let (mut s, id) = super::super::tests::setup();
    let ctx = context(&s, id).unwrap();
    let target = person("UCALRETAINED", "Previously selected profile", Some(true));
    assert!(
        !s.core.workspaces[&ctx.session.team_id]
            .users
            .contains_key(&target.id)
    );
    let c = active_mut(&mut s, id).unwrap();
    c.share_channel = target.id.clone();
    c.share_selected = Some(Destination::User(target.clone()));
    c.share_query = "no matching name".into();
    c.share_generation = 19;
    c.share_loading = true;
    assert_eq!(resolve(&s, id).as_deref(), Some(""));
    publish(
        &mut s,
        id,
        19,
        &ctx,
        "no matching name",
        vec![Destination::User(person(
            "UCALSIBLING",
            "Directory Sibling",
            None,
        ))],
        false,
    );
    let c = active(&s, id).unwrap();
    assert_eq!(
        c.share_choices
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        [target.id.as_str(), "UCALSIBLING"]
    );
    assert_eq!(
        c.share_remote
            .iter()
            .map(Destination::id)
            .collect::<Vec<_>>(),
        ["UCALSIBLING"]
    );
    assert_eq!(c.share_channel, target.id);
    assert_eq!(resolve(&s, id).as_deref(), Some(""));
    assert!(!c.share_loading);
    assert!(
        !s.core.workspaces[&ctx.session.team_id]
            .users
            .contains_key(&target.id)
    );
}
