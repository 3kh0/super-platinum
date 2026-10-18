use super::*;
use serde_json::json;
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TONE".into(),
        user_id: "USELF".into(),
        enterprise_id: None,
        name: "Test".into(),
        url: "https://example.invalid".into(),
        token: "synthetic".into(),
    }
}
fn facts() -> Facts {
    Facts {
        self_id: "USELF".into(),
        current_team: "TONE".into(),
        omit_team: "TONE".into(),
        retain_public_membership_on_archive: false,
        members: BTreeMap::new(),
    }
}
fn row(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"is_channel":true})
}
fn reply(p: &Pending) -> RendererReply {
    RendererReply {
        generation: p.generation().into(),
        fields: p.fields().to_vec(),
        values: p.fields().iter().map(|s| s.to_lowercase()).collect(),
    }
}
fn settle(s: &mut Store, native: &mut BTreeMap<String, Channel>) {
    let p = s.prepare("A", &owner(), 1, "Q1", native, facts()).unwrap();
    let r = reply(&p);
    let c = s
        .finish(p, &facts(), r, "A", &owner(), 1, "Q1", native)
        .unwrap();
    (*s, *native) = c.into_parts();
}
fn observed_store() -> Store {
    let mut s = Store::default();
    let lease = s.begin_full_boot("A", &owner(), 1).unwrap();
    assert!(s.finish_full_boot(lease, "A", &owner(), 1));
    s
}
fn seeded() -> (Store, BTreeMap<String, Channel>) {
    let mut s = observed_store();
    let mut native = BTreeMap::new();
    s.begin("A", &owner(), &native).unwrap();
    s.enqueue(
        "A",
        &owner(),
        1,
        Origin::Directory,
        vec![row("CZ", "Z"), row("CA", "A")],
    )
    .unwrap();
    settle(&mut s, &mut native);
    (s, native)
}
#[test]
fn actual_processor_vectors_match_canonical_rows() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("source-vectors.json")).unwrap();
    for case in cases {
        let c = &case["context"];
        let f = Facts {
            self_id: c["selfId"].as_str().unwrap().into(),
            current_team: c["currentTeam"].as_str().unwrap().into(),
            omit_team: c["omitTeam"].as_str().unwrap().into(),
            retain_public_membership_on_archive: c["retainArchived"].as_bool().unwrap(),
            members: serde_json::from_value(c["members"].clone()).unwrap(),
        };
        let before = case["before"].as_array().unwrap();
        let incoming = case["incoming"].as_array().unwrap();
        let p = plan::Plan::capture(
            f.clone(),
            c["bootInFlight"].as_bool().unwrap(),
            "source-vector",
            before,
            incoming,
        )
        .unwrap();
        let values = p
            .fields()
            .iter()
            .map(|s| case["lowercase"][s].as_str().unwrap().to_owned())
            .collect();
        let r = RendererReply {
            generation: p.generation().into(),
            fields: p.fields().to_vec(),
            values,
        };
        let actual = p.apply(&f, r).unwrap();
        assert_eq!(
            actual.rows(),
            case["rows"].as_array().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            actual.updates(),
            case["updates"].as_array().unwrap(),
            "{}",
            case["name"]
        );
    }
}
#[test]
fn encounter_order_update_duplicate_remove_reinsert_are_atomic() {
    let (mut s, mut native) = seeded();
    s.enqueue(
        "A",
        &owner(),
        1,
        Origin::Directory,
        vec![row("CB", "first"), row("CZ", "changed"), row("CB", "last")],
    )
    .unwrap();
    settle(&mut s, &mut native);
    let ready = s.ready("A", &owner(), 1, "Q1", &native).unwrap();
    assert_eq!(
        ready
            .rows()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["CZ", "CA", "CB"]
    );
    assert_eq!(ready.rows()[2]["name"], "last");
    (s, native) = s
        .remove("A", &owner(), &BTreeSet::from(["CZ".into()]), &native)
        .unwrap()
        .into_parts();
    assert!(!ready.current(&s, "A", &owner(), 1, "Q1", &native));
    assert!(!native.contains_key("CZ"));
    s.enqueue("A", &owner(), 1, Origin::Direct, vec![row("CZ", "again")])
        .unwrap();
    settle(&mut s, &mut native);
    assert_eq!(
        s.ready("A", &owner(), 1, "Q1", &native).unwrap().rows()[2]["id"],
        "CZ"
    );
}
#[test]
fn native_read_state_is_preserved_without_filling_source_fields() {
    let (mut s, mut native) = seeded();
    let mut raw = serde_json::to_value(&native["CZ"]).unwrap();
    raw["latest"] = json!({"ts":"9.1"});
    raw["last_read"] = json!("8.1");
    native.insert("CZ".into(), serde_json::from_value(raw).unwrap());
    assert!(s.ready("A", &owner(), 1, "Q1", &native).is_ok());
    s.enqueue("A", &owner(), 1, Origin::Direct, vec![row("CZ", "Z")])
        .unwrap();
    settle(&mut s, &mut native);
    let raw = serde_json::to_value(&native["CZ"]).unwrap();
    assert_eq!(raw["last_read"], "8.1");
    assert_eq!(raw["latest"]["ts"], "9.1");
    let ready = s.ready("A", &owner(), 1, "Q1", &native).unwrap();
    assert!(ready.rows()[0].get("latest").is_none());
    assert!(ready.rows()[0].get("last_read").is_none());
}
#[test]
fn stale_owner_context_renderer_and_branch_cannot_mint_commit() {
    let (mut s, native) = seeded();
    s.enqueue("A", &owner(), 1, Origin::Direct, vec![row("CZ", "new")])
        .unwrap();
    let p = s.prepare("A", &owner(), 1, "Q1", &native, facts()).unwrap();
    let r = reply(&p);
    let mut rotated = owner();
    rotated.token = "new-synthetic".into();
    assert!(
        s.finish(p, &facts(), r, "A", &rotated, 1, "Q1", &native)
            .is_err()
    );
    let p = s.prepare("A", &owner(), 1, "Q1", &native, facts()).unwrap();
    let r = reply(&p);
    let mut changed = facts();
    changed.omit_team = "TOTHER".into();
    assert!(
        s.finish(p, &changed, r, "A", &owner(), 1, "Q1", &native)
            .is_err()
    );
    let p = s.prepare("A", &owner(), 1, "Q1", &native, facts()).unwrap();
    let mut r = reply(&p);
    r.generation = "Q2".into();
    assert!(
        s.finish(p, &facts(), r, "A", &owner(), 1, "Q1", &native)
            .is_err()
    );
    let p = s.prepare("A", &owner(), 1, "Q1", &native, facts()).unwrap();
    let r = reply(&p);
    let mut branch = s.clone();
    branch.invalidate("CA");
    assert!(
        branch
            .finish(p, &facts(), r, "A", &owner(), 1, "Q1", &native)
            .is_err()
    );
}
#[test]
fn legacy_unknown_cache_roundtrip_and_account_namespace_remain_distinct() {
    let (s, native) = seeded();
    let raw = s.cached().unwrap();
    let restored = Store::restore(Some(&raw), &owner());
    assert!(restored.ready("A", &owner(), 1, "Q1", &native).is_ok());
    assert!(restored.ready("B", &owner(), 1, "Q1", &native).is_err());
    let mut foreign = restored.clone();
    assert!(
        foreign
            .observe_boot("B", &owner(), &native, 1, &json!({"channels":[]}))
            .is_err()
    );
    assert_eq!(foreign.cached(), restored.cached());
    assert!(
        Store::restore(None, &owner())
            .ready("A", &owner(), 1, "Q1", &native)
            .is_err()
    );
    assert!(
        Store::default()
            .ready("A", &owner(), 1, "Q1", &BTreeMap::new())
            .is_err()
    );
    let mut malformed = raw;
    malformed["records"][0]["native"]["id"] = json!("CWRONG");
    assert!(
        Store::restore(Some(&malformed), &owner())
            .cached()
            .is_none()
    );
}
#[test]
fn native_boot_preserves_received_array_order_and_first_duplicate() {
    let mut s = observed_store();
    let _boot = s.begin_full_boot("A", &owner(), 1).unwrap();
    let mut native = BTreeMap::new();
    s.observe_boot("A",&owner(),&native,1,&json!({"channels":[row("CZ","first"),row("CA","A")],"groups":[row("CZ","ignored"),row("GB","G")]})).unwrap();
    settle(&mut s, &mut native);
    let ready = s.ready("A", &owner(), 1, "Q1", &native).unwrap();
    assert_eq!(
        ready
            .rows()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["CZ", "CA", "GB"]
    );
    assert_eq!(ready.rows()[0]["name"], "first");
    s.observe_boot("A", &owner(), &native, 1, &json!({"channels":[]}))
        .unwrap();
    assert_eq!(
        s.ready("A", &owner(), 1, "Q1", &native)
            .unwrap()
            .rows()
            .len(),
        3
    );
}

#[test]
fn actual_bootread_acquires_only_at_current_workspace_adoption() {
    use crate::{
        CoreAppState,
        config::Session,
        slack::Transport,
        state::{Workspace, posting_metadata::authority::BootRead},
    };
    let mut core = CoreAppState::new(Default::default());
    let session = owner();
    core.active_account = Some("A".into());
    let transport = Arc::new(Transport::disconnected_fixture("synthetic-cookie"));
    core.transport = Some(transport.clone());
    core.workspaces
        .insert(session.team_id.clone(), Workspace::from_session(&session));
    core.session = Some(Session {
        d_cookie: "synthetic-cookie".into(),
        workspaces: BTreeMap::from([(session.team_id.clone(), session.clone())]),
        unified_user_client_enabled: BTreeMap::from([(session.team_id.clone(), true)]),
    });
    let raw = json!({"ok":true,"team":{"id":"TONE"},"self":{"id":"USELF"},
        "channels":[row("CZ","Z"),row("CA","A")],"users":[]});
    let read = BootRead::begin(&mut core, &transport, &session).unwrap();
    assert!(read.apply(&mut core, raw.clone(), false).unwrap());
    assert!(
        core.workspaces["TONE"]
            .directory_channels
            .cached()
            .is_none()
    );
    assert!(
        core.workspaces["TONE"]
            .directory_channels
            .lifecycle
            .phase("A", &session, core.account_epoch)
            .is_err()
    );
    let read = BootRead::begin_full(&mut core, &transport, &session).unwrap();
    assert_eq!(
        core.workspaces["TONE"].directory_channels.lifecycle.phase(
            "A",
            &session,
            core.account_epoch
        ),
        Ok(true)
    );
    assert!(read.apply(&mut core, raw.clone(), true).unwrap());
    assert_eq!(
        core.workspaces["TONE"].directory_channels.lifecycle.phase(
            "A",
            &session,
            core.account_epoch
        ),
        Ok(false)
    );
    let w = core.workspaces.get_mut("TONE").unwrap();
    assert!(w.directory_channels.has_pending());
    settle(&mut w.directory_channels, &mut w.channels);
    assert_eq!(
        w.directory_channels
            .ready("A", &session, 1, "Q1", &w.channels)
            .unwrap()
            .rows()[0]["id"],
        "CZ"
    );
    let before = core.workspaces["TONE"].directory_channels.cached();
    let read = BootRead::begin(&mut core, &transport, &session).unwrap();
    core.account_epoch += 1;
    assert!(!read.apply(&mut core, raw, true).unwrap());
    assert_eq!(core.workspaces["TONE"].directory_channels.cached(), before);
    let cancelled = BootRead::begin_full(&mut core, &transport, &session).unwrap();
    drop(cancelled);
    assert!(
        core.workspaces["TONE"]
            .directory_channels
            .lifecycle
            .phase("A", &session, core.account_epoch)
            .is_err()
    );
    let failed = BootRead::begin_full(&mut core, &transport, &session).unwrap();
    assert!(failed.failed(&mut core));
    assert_eq!(
        core.workspaces["TONE"].directory_channels.lifecycle.phase(
            "A",
            &session,
            core.account_epoch
        ),
        Ok(false)
    );
    for raw in [
        json!({"ok":false}),
        json!({"ok":true,"self":42}),
        json!({"ok":true,"team":{"id":"TOTHER"},"self":{"id":"USELF"}}),
    ] {
        let malformed = BootRead::begin_full(&mut core, &transport, &session).unwrap();
        assert!(malformed.apply(&mut core, raw, true).is_err());
        assert_eq!(
            core.workspaces["TONE"].directory_channels.lifecycle.phase(
                "A",
                &session,
                core.account_epoch
            ),
            Ok(false)
        );
        assert_eq!(core.workspaces["TONE"].directory_channels.cached(), before);
    }
    let retired = BootRead::begin_full(&mut core, &transport, &session).unwrap();
    let successor = BootRead::begin_full(&mut core, &transport, &session).unwrap();
    assert!(!retired.apply(&mut core, json!({"ok":false}), true).unwrap());
    assert_eq!(
        core.workspaces["TONE"].directory_channels.lifecycle.phase(
            "A",
            &session,
            core.account_epoch
        ),
        Ok(true)
    );
    drop(successor);
    assert!(
        core.workspaces["TONE"]
            .directory_channels
            .lifecycle
            .phase("A", &session, core.account_epoch)
            .is_err()
    );
}

#[test]
fn delayed_hydration_uses_ingress_phase_after_boot_completion_or_new_boot() {
    for ingress_booting in [true, false] {
        let (mut s, mut native) = seeded();
        s.enqueue(
            "A",
            &owner(),
            1,
            Origin::Direct,
            vec![json!({"id":"CZ","is_migrating":true})],
        )
        .unwrap();
        settle(&mut s, &mut native);
        let lease = if ingress_booting {
            Some(s.begin_full_boot("A", &owner(), 1).unwrap())
        } else {
            None
        };
        s.enqueue(
            "A",
            &owner(),
            1,
            Origin::Boot,
            vec![json!({"id":"CZ","name":"after"})],
        )
        .unwrap();
        let _later_boot = if let Some(lease) = lease {
            assert!(s.finish_full_boot(lease, "A", &owner(), 1));
            None
        } else {
            Some(s.begin_full_boot("A", &owner(), 1).unwrap())
        };
        let saved = s.cached().unwrap();
        // A warm restore has no current lifecycle authority. Its retained
        // accepted batch still carries its original observed phase.
        s = Store::restore(Some(&saved), &owner());
        assert!(s.lifecycle.phase("A", &owner(), 1).is_err());
        settle(&mut s, &mut native);
        let rows = s.ready("A", &owner(), 1, "Q1", &native).unwrap();
        assert_eq!(rows.rows()[0]["is_migrating"], !ingress_booting);
    }
}

#[test]
fn absent_lifecycle_cannot_enqueue_and_legacy_pending_phase_stays_unknown() {
    let mut absent = Store::default();
    absent.begin("A", &owner(), &BTreeMap::new()).unwrap();
    assert!(
        absent
            .enqueue("A", &owner(), 1, Origin::Directory, vec![row("CZ", "Z")])
            .is_err()
    );
    assert!(!absent.has_pending());
    let (mut s, native) = seeded();
    s.enqueue(
        "A",
        &owner(),
        1,
        Origin::Directory,
        vec![row("CZ", "after")],
    )
    .unwrap();
    let mut old = s.cached().unwrap();
    old["pending"][0]
        .as_object_mut()
        .unwrap()
        .remove("boot_in_flight");
    let restored = Store::restore(Some(&old), &owner());
    assert!(
        restored
            .prepare("A", &owner(), 1, "Q1", &native, facts())
            .is_err()
    );
    assert!(restored.has_pending());
    assert!(!format!("{s:?}").contains("synthetic"));
    assert!(!s.cached().unwrap().to_string().contains("synthetic"));
}
