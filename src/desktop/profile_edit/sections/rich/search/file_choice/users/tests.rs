use super::*;
use std::collections::BTreeMap;
use super_platinum_core::{
    block_actions::{name_policy, workflow_management::member_hydration::Flags},
    config::{Session, WorkspaceSession},
    slack::Transport,
    state::{Workspace, directory_members::Origin},
};
const TEAM: &str = "TTITLE01";
const MEMBER: &str = "UTITLE01";
const FILE: &str = "FTITLE01";
fn shell() -> ShellState {
    let mut s = ShellState::login(crate::media::MediaRegistry::default());
    let owner = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "USELF01".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    s.core.active_account = Some("title-fixture".into());
    s.core.active_team = Some(TEAM.into());
    s.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(TEAM.into(), owner.clone())]),
    });
    s.core.transport = Some(Arc::new(Transport::disconnected_fixture("synthetic")));
    let mut w = Workspace::from_session(&owner);
    w.directory_members
        .begin("title-fixture", &owner, &BTreeSet::new())
        .unwrap();
    let raw = json!({"id":MEMBER,"name":"ignored","updated":1,"profile":{"display_name":"  Cafe\u{301}  ","real_name":"Real"},"opaque":17});
    w.directory_members
        .enqueue(
            "title-fixture",
            &owner,
            Origin::Directory,
            vec![
                raw,
                json!({"id":"UEXTRA01","name":"extra","profile":{"display_name":"Extra"}}),
            ],
            json!({(MEMBER):true,"UEXTRA01":true}),
        )
        .unwrap();
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let pending = w
        .directory_members
        .prepare(
            "title-fixture",
            &owner,
            s.core.account_epoch,
            "title",
            flags.clone(),
            &w.users,
        )
        .unwrap();
    let reply = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending.fields().iter().map(|s| s.to_lowercase()).collect(),
    };
    w.users = w
        .directory_members
        .adopt(
            pending,
            reply,
            &flags,
            "title-fixture",
            &owner,
            s.core.account_epoch,
            "title",
            &w.users,
        )
        .unwrap()
        .into_parts()
        .1;
    s.core.workspaces.insert(TEAM.into(), w);
    s
}
fn observed(s: &ShellState) -> Users {
    let owner = Arc::new(Context::capture(s, TEAM).unwrap());
    let w = &s.core.workspaces[TEAM];
    let generation = "private-title".to_owned();
    let subset = w
        .directory_members
        .subset(
            &owner.account,
            &owner.session,
            owner.epoch,
            &generation,
            &BTreeSet::from([MEMBER.into(), "UEXTRA01".into()]),
            &w.users,
        )
        .unwrap();
    Users {
        owner,
        subset,
        generation,
    }
}
fn policy(hidden: &str) -> name_policy::Snapshot {
    let membership = name_policy::Membership::observe(
        None,
        &[TEAM.into()],
        &BTreeMap::from([(TEAM.into(), json!({"id":TEAM}))]),
    )
    .unwrap();
    let user = name_policy::UserPrefs::decode(
        &json!({"ok":true,"prefs":{"hidden_users":hidden,"display_real_names_override":1}}),
    )
    .unwrap();
    name_policy::Snapshot::new(name_policy::OrdinaryClientV2, membership, user, None).unwrap()
}

#[test]
fn display_preimage_recovers_invalidated_required_member_without_query_ready() {
    let mut s = shell();
    let owner = Context::capture(&s, TEAM).unwrap();
    s.core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .directory_members
        .invalidate(MEMBER);
    let generation = "independent-display-recovery";
    let required = BTreeSet::from([MEMBER.into()]);
    let source = display::Source::capture(&s, TEAM, generation, &required).unwrap();
    assert!(source.current(&s));
    assert_eq!(
        source.unavailable_known(&s, &required).unwrap(),
        vec![MEMBER.to_owned()]
    );
    let w = &s.core.workspaces[TEAM];
    let observations = w
        .directory_members
        .observations(&owner.account, &owner.session, owner.epoch, generation)
        .unwrap();
    assert!(super_platinum_core::block_actions::directory_endpoint::member_info::unknown::Capture::network_only(observations, &[MEMBER.into()]).unwrap().ids().is_empty(), "retained invalidated source must use known recovery");
    assert!(
        w.directory_members
            .ready(
                &owner.account,
                &owner.session,
                owner.epoch,
                generation,
                &w.users
            )
            .is_err()
    );
    assert!(Users::capture_owned(&s, TEAM, generation, &required).is_err());
    // Real canonical adoption, not invalidation removal or an injected title.
    let w = s.core.workspaces.get_mut(TEAM).unwrap();
    w.directory_members.enqueue_member_info(&owner.account, &owner.session,
        vec![json!({"id":MEMBER,"name":"recovered","updated":2,"profile":{"display_name":"Recovered title"},"opaque":43,"is_stranger":false})], json!({(MEMBER):true}), vec![MEMBER.into()], vec![]).unwrap();
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let pending = w
        .directory_members
        .prepare(
            &owner.account,
            &owner.session,
            owner.epoch,
            generation,
            flags.clone(),
            &w.users,
        )
        .unwrap();
    let reply = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending.fields().iter().map(|s| s.to_lowercase()).collect(),
    };
    w.users = w
        .directory_members
        .adopt(
            pending,
            reply,
            &flags,
            &owner.account,
            &owner.session,
            owner.epoch,
            generation,
            &w.users,
        )
        .unwrap()
        .into_parts()
        .1;
    assert!(
        !source.current(&s),
        "the stale preimage cannot authorize a later install"
    );
    let accepted = display::Source::capture(&s, TEAM, generation, &required).unwrap();
    let users = Users::capture_owned(&s, TEAM, generation, &required).unwrap();
    assert!(accepted.current(&s));
    assert!(
        accepted
            .unavailable_known(&s, &required)
            .unwrap()
            .is_empty()
    );
    assert!(users.current(&s));
    assert_eq!(
        users.label(MEMBER, &policy("")).unwrap(),
        "@Recovered title"
    );
    assert_eq!(users.snapshot(&s)["members"][0]["source"]["opaque"], 43);
    assert!(
        !s.core.workspaces[TEAM]
            .directory_members
            .invalidated()
            .contains(MEMBER)
    );
}

#[test]
fn invalidated_display_preimage_refuses_owner_native_and_canonical_replacement() {
    for mutation in 0..5 {
        let mut s = shell();
        s.core
            .workspaces
            .get_mut(TEAM)
            .unwrap()
            .directory_members
            .invalidate(MEMBER);
        let source =
            display::Source::capture(&s, TEAM, "display-held", &BTreeSet::from([MEMBER.into()]))
                .unwrap();
        assert!(source.current(&s));
        match mutation {
            0 => s.core.active_account = Some("successor".into()),
            1 => {
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .unified_user_client_enabled
                    .insert(TEAM.into(), true);
            }
            2 => s.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
            3 => {
                s.core
                    .workspaces
                    .get_mut(TEAM)
                    .unwrap()
                    .users
                    .get_mut(MEMBER)
                    .unwrap()
                    .name = Some("successor".into())
            }
            _ => s
                .core
                .workspaces
                .get_mut(TEAM)
                .unwrap()
                .directory_members
                .invalidate("UEXTRA01"),
        }
        assert!(
            !source.current(&s),
            "mutation {mutation} must refuse original held adoption"
        );
        assert!(
            Users::capture_owned(&s, TEAM, "display-held", &BTreeSet::from([MEMBER.into()]))
                .is_err()
        );
    }
}
#[test]
fn display_native_preimage_tracks_required_rows_and_explicit_absence_only() {
    let mut s = shell();
    let required = BTreeSet::from([MEMBER.into(), "UABSENT01".into()]);
    let source = display::Source::capture(&s, TEAM, "bounded-display", &required).unwrap();
    s.core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .users
        .get_mut("UEXTRA01")
        .unwrap()
        .name = Some("unrelated typed update".into());
    assert!(
        source.current(&s),
        "unrelated typed rows are not title dependencies"
    );
    s.core.workspaces.get_mut(TEAM).unwrap().users.insert(
        "UABSENT01".into(),
        serde_json::from_value(json!({"id":"UABSENT01","name":"new"})).unwrap(),
    );
    assert!(
        !source.current(&s),
        "required typed-only arrival retires explicit absence"
    );
}
fn file(id: &str) -> Value {
    json!({"id":id,"mode":"quip","filetype":"quip","created":1,"title":"Plain matching title",
        "permalink":"https://fixture.invalid/exact?raw=1&keep=2","opaque":42,
        "title_blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":"Review "},{"type":"user","user_id":MEMBER},
            {"type":"text","text":" / "},{"type":"user","user_id":MEMBER}]}]}]})
}
#[test]
fn observed_user_title_bypasses_quip_without_changing_exact_insertion_wire() {
    let mut s = shell();
    s.canvas_titles = quip_titles::State::default();
    let users = observed(&s);
    let raw = file(FILE);
    let mut titles = file_title_source::Store::new();
    titles.ingest(&raw).unwrap();
    for (hidden, label) in [
        ("", "Review @  Cafe\u{301}   / @  Cafe\u{301}  "),
        (MEMBER, "Review Hidden user / Hidden user"),
    ] {
        let result = Projection::capture_with_users(
            &s,
            TEAM,
            "USELF01",
            &json!({"ok":true,"workspaces":[{"id":TEAM}]}),
            titles.row(FILE).unwrap(),
            FILE,
            Some((&users, &policy(hidden))),
        )
        .unwrap();
        assert_eq!(result.label, label);
        assert!(result.quip.is_none());
        assert_eq!(
            result.value,
            json!({"type":"canvas","file_id":FILE,"text":raw["permalink"],"url":raw["permalink"]})
        );
        assert_eq!(result._title.source(), &raw);
    }
    assert_eq!(users.snapshot(&s)["members"][0]["source"]["opaque"], 17);
}
#[test]
fn private_title_witness_refuses_member_native_account_auth_and_socket_replacement() {
    for mutation in 0..5 {
        let mut s = shell();
        let users = observed(&s);
        assert!(users.current(&s));
        match mutation {
            0 => {
                s.core
                    .workspaces
                    .get_mut(TEAM)
                    .unwrap()
                    .users
                    .get_mut(MEMBER)
                    .unwrap()
                    .name = Some("changed".into())
            }
            1 => s.core.active_account = Some("successor".into()),
            2 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .unified_user_client_enabled
                .insert(TEAM.into(), true)
                .map(|_| ())
                .unwrap_or(()),
            3 => s.core.workspaces.get_mut(TEAM).unwrap().rt_generation += 1,
            _ => {
                let owner = users.owner.session.clone();
                s.core
                    .workspaces
                    .get_mut(TEAM)
                    .unwrap()
                    .directory_members
                    .enqueue(
                        "title-fixture",
                        &owner,
                        Origin::DirectUpsert,
                        vec![json!({"id":MEMBER,"name":"new"})],
                        json!({}),
                    )
                    .unwrap();
            }
        }
        assert!(!users.current(&s), "mutation {mutation}");
    }
}
#[test]
fn only_matched_canvas_rows_supply_title_dependencies() {
    let mut s = shell();
    let source = file_source::fixture_target_source(&mut s, TEAM, vec![file(FILE)]).unwrap();
    assert!(ids(&source, &[]).unwrap().is_empty());
    assert_eq!(
        ids(&source, &[FILE.into()]).unwrap(),
        BTreeSet::from([MEMBER.into()])
    );
    assert!(ids(&source, &["FMISSING01".into()]).is_err());
    let mut changed = file(FILE);
    changed["title"] = json!("Successor title");
    let successor = file_source::fixture_target_source(&mut s, TEAM, vec![changed]).unwrap();
    assert!(!source.current(&s));
    assert!(successor.current(&s));
    assert_eq!(
        source.title(FILE).unwrap().effective()["title"],
        "Plain matching title"
    );
}

#[test]
fn private_choice_users_are_scoped_to_actual_canvas_dependencies() {
    let s = shell();
    let available = observed(&s);
    assert_eq!(available.subset.rows().len(), 2);
    let user_canvas = file(FILE);
    let users = for_title(&s, &user_canvas, Some(&available))
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&users.owner, &available.owner));
    assert_eq!(users.snapshot(&s)["members"].as_array().unwrap().len(), 1);
    assert_eq!(users.snapshot(&s)["members"][0]["source"]["id"], MEMBER);
    let mut plain_canvas = file(FILE);
    plain_canvas["title_blocks"] = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Plain"}]}]}]);
    let mut list = file("FLIST01");
    list["mode"] = json!("list");
    list["filetype"] = json!("list");
    for raw in [&plain_canvas, &list] {
        assert!(for_title(&s, raw, Some(&available)).unwrap().is_none());
        assert!(for_title(&s, raw, None).unwrap().is_none());
    }
    assert!(for_title(&s, &user_canvas, None).is_err());
    let mut foreign = user_canvas.clone();
    foreign["title_blocks"][0]["elements"][0]["elements"][1]["user_id"] = json!("UOTHER01");
    assert!(for_title(&s, &foreign, Some(&available)).is_err());
}

#[test]
fn accepted_member_intake_retires_preimage_but_retains_advanced_title_lineage() {
    let mut s = shell();
    let before = observed(&s);
    let owner = before.owner.session.clone();
    let epoch = before.owner.epoch;
    let w = s.core.workspaces.get_mut(TEAM).unwrap();
    w.directory_members.enqueue("title-fixture", &owner, Origin::DirectUpsert,
        vec![json!({"id":MEMBER,"name":"changed","updated":2,"profile":{"display_name":"Recovered title"}})],json!({})).unwrap();
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let pending = w
        .directory_members
        .prepare(
            "title-fixture",
            &owner,
            epoch,
            "owned-recovery",
            flags.clone(),
            &w.users,
        )
        .unwrap();
    let reply = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending.fields().iter().map(|s| s.to_lowercase()).collect(),
    };
    w.users = w
        .directory_members
        .adopt(
            pending,
            reply,
            &flags,
            "title-fixture",
            &owner,
            epoch,
            "owned-recovery",
            &w.users,
        )
        .unwrap()
        .into_parts()
        .1;
    let advanced = observed(&s);
    assert!(!before.current(&s));
    assert!(advanced.current(&s));
    assert!(for_title(&s, &file(FILE), Some(&before)).is_err());
    let choice = for_title(&s, &file(FILE), Some(&advanced))
        .unwrap()
        .unwrap();
    assert_eq!(
        choice.label(MEMBER, &policy("")).unwrap(),
        "@Recovered title"
    );
    // A later independent replacement still retires the advanced receipt.
    s.core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .users
        .get_mut(MEMBER)
        .unwrap()
        .name = Some("successor".into());
    assert!(!advanced.current(&s));
    assert!(!choice.current(&s));
}

#[test]
fn independently_owned_existing_titles_preserve_raw_and_retire_original_sources() {
    use std::sync::atomic::{AtomicBool, Ordering};
    for mutation in 0..4 {
        let mut s = shell();
        file_source::fixture_target_source(&mut s, TEAM, vec![file(FILE)]).unwrap();
        let raw = json!({"type":"canvas","file_id":FILE,"url":"https://fixture.invalid/exact","text":"wire only","style":{"bold":true},"opaque":{"keep":[1,2]}});
        let live = Arc::new(AtomicBool::new(true));
        let owner = live.clone();
        let files = FileTitles::capture(
            &s,
            TEAM,
            vec![raw.clone()],
            Arc::new(move |_| owner.load(Ordering::SeqCst)),
        )
        .unwrap();
        let users = Arc::new(
            Users::capture_owned(&s, TEAM, "independent-list-display", files.required()).unwrap(),
        );
        let titles = Arc::new(
            files
                .complete(&s, Some(users), Some(policy("")), Arc::new(|_| true))
                .unwrap(),
        );
        assert_eq!(
            titles.label(&s, &raw).unwrap(),
            "Review @  Cafe\u{301}   / @  Cafe\u{301}  "
        );
        assert!(
            FileTarget::capture(&s, TEAM, &raw).is_err(),
            "no implicit display authority"
        );
        let target = FileTarget::capture_with_titles(&s, TEAM, &raw, Some(titles.clone())).unwrap();
        assert_eq!(target.raw(), &raw);
        let mut duplicate = raw.clone();
        duplicate["style"] = json!({"italic":true});
        duplicate["opaque"] = json!({"second":2});
        let second =
            FileTarget::capture_with_titles(&s, TEAM, &duplicate, Some(titles.clone())).unwrap();
        assert_eq!(second.raw(), &duplicate);
        assert_eq!(second.label(), target.label());
        duplicate["record_id"] = json!("unowned-subobject");
        assert!(
            FileTarget::capture_with_titles(&s, TEAM, &duplicate, Some(titles.clone())).is_err()
        );
        match mutation {
            0 => live.store(false, Ordering::SeqCst),
            1 => {
                s.core
                    .workspaces
                    .get_mut(TEAM)
                    .unwrap()
                    .users
                    .get_mut(MEMBER)
                    .unwrap()
                    .name = Some("successor".into())
            }
            2 => file_source::invalidate(&mut s, TEAM, FILE),
            _ => s.core.account_epoch += 1,
        }
        assert!(!titles.current(&s));
        assert!(!target.current(&s));
        assert!(titles.labels(&s).is_err());
    }
}
#[test]
fn draft_dependency_gate_skips_unsupported_and_plain_tokens_without_altering_bytes() {
    let mut s = shell();
    let mut plain = file("FPLAIN01");
    plain.as_object_mut().unwrap().remove("title_blocks");
    let list =
        json!({"id":"FLIST01","mode":"list","filetype":"list","title":"Plain List","created":1});
    file_source::fixture_target_source(&mut s, TEAM, vec![file(FILE), plain, list]).unwrap();
    let wanted = json!({"type":"canvas","file_id":FILE,"text":"wire","opaque":17});
    let mut sub = wanted.clone();
    sub["section_id"] = json!("section");
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[wanted.clone(),sub,
        {"type":"file","file_id":"FHOSTED","opaque":42},{"type":"canvas","file_id":"FUNKNOWN"},
        {"type":"canvas","file_id":"FPLAIN01"},{"type":"file","file_id":"FLIST01"}]}]}),
    ];
    let before = blocks.clone();
    assert_eq!(
        FileTitles::dependent_tokens(&s, TEAM, &blocks),
        vec![wanted]
    );
    assert_eq!(blocks, before);
    assert_eq!(FileTitles::tokens(&blocks).len(), 6);
}

mod empty_reply;
mod empty_transcript;
