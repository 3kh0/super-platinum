use super::*;
use serde_json::json;

fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T_GROUP_TEST".into(),
        user_id: "U_SELF".into(),
        enterprise_id: None,
        name: "Test".into(),
        url: "https://fixture.test".into(),
        token: "synthetic-secret-never-saved".into(),
    }
}
fn boot() -> Value {
    json!({"ok":true,"team":{"id":"T_GROUP_TEST"},"self":{"id":"U_SELF"},"subteams":{"self":[]}})
}
fn ready() -> (Store, Native) {
    let store = Store::fresh();
    let native = Native::new();
    store
        .observe_boot(
            store.begin_boot(&native).unwrap(),
            "account",
            &owner(),
            1,
            &native,
            &boot(),
        )
        .unwrap()
        .into_parts()
}
fn op(rows: Value) -> Operation {
    Operation::Metadata {
        origin: Origin::Fetch,
        rows: rows.as_array().unwrap().clone(),
        native_members: NativeMembers::Preserve,
    }
}
fn enqueue(store: &mut Store, native: &Native, operation: Operation) -> Ticket {
    store
        .enqueue("account", &owner(), 1, "test", native, operation)
        .unwrap()
}
fn finish(store: &Store, native: &Native, ticket: &Ticket) -> Commit {
    let pending = store
        .next_for(ticket, "account", &owner(), 1, native)
        .unwrap()
        .unwrap();
    let reply = RendererReply {
        generation: pending.generation().into(),
        fields: pending.fields().to_vec(),
        lowered: pending.fields().iter().map(|v| v.to_lowercase()).collect(),
        locale: (!pending.fields().is_empty()).then(|| "en-US".into()),
    };
    store
        .commit(pending, reply, "account", &owner(), 1, native)
        .unwrap()
}
fn seed() -> (Store, Native) {
    let (mut s, n) = ready();
    let ticket = enqueue(
        &mut s,
        &n,
        op(
            json!([{"id":"S1","name":"Before","handle":"before","date_update":101,"opaque":{"keep":true},"users":["U_SOURCE"]}]),
        ),
    );
    finish(&s, &n, &ticket).into_parts()
}

#[test]
fn only_actual_fresh_full_boot_binds_and_membership_is_not_metadata() {
    assert!(Store::default().begin_boot(&Native::new()).is_err());
    let s = Store::fresh();
    let n = Native::new();
    let mut raw = boot();
    raw["subteams"]["self"] = json!(["S1"]);
    let (s, n) = s
        .observe_boot(s.begin_boot(&n).unwrap(), "account", &owner(), 1, &n, &raw)
        .unwrap()
        .into_parts();
    let snap = s.snapshot("account", &owner(), 1, &n).unwrap();
    assert!(snap.rows().is_empty());
    assert_eq!(snap.membership().unwrap(), ["S1"]);
    assert_eq!(n["S1"].users, ["U_SELF"]);
    assert!(n["S1"].label_source.is_none());
}
#[test]
fn boot_original_native_and_revision_fences_and_wrong_owner_refuse() {
    let (mut s, mut n) = ready();
    let lease = s.begin_boot(&n).unwrap();
    let ticket = enqueue(&mut s, &n, op(json!([{"id":"S1","name":"New"}])));
    assert!(
        s.observe_boot(lease, "account", &owner(), 1, &n, &boot())
            .is_err()
    );
    let (s, next) = finish(&s, &n, &ticket).into_parts();
    n = next;
    let lease = s.begin_boot(&n).unwrap();
    n.get_mut("S1").unwrap().handle = "untracked".into();
    assert!(
        s.observe_boot(lease, "account", &owner(), 1, &n, &boot())
            .is_err()
    );
}
#[test]
fn fifo_duplicates_share_prebatch_predecessor_and_reselect_final() {
    let (mut s, n) = seed();
    let t = enqueue(
        &mut s,
        &n,
        op(
            json!([{"id":"S1","name":"First","firstOnly":true,"date_update":102},{"id":"S1","handle":"last","date_update":103}]),
        ),
    );
    let commit = finish(&s, &n, &t);
    assert_eq!(commit.results().len(), 2);
    assert_eq!(commit.results()[0].effective, commit.results()[1].effective);
    let e = &commit.results()[0].effective;
    assert_eq!(e["name"], "Before");
    assert_eq!(e["handle"], "last");
    assert_eq!(e["date_update"], 103);
    assert!(e.get("firstOnly").is_none());
    assert!(e.get("_name_lc").is_none());
    assert_eq!(e["opaque"], json!({"keep":true}));
    let (s, n) = commit.into_parts();
    assert_eq!(
        s.snapshot("account", &owner(), 1, &n).unwrap().rows()[0].source["handle"],
        "last"
    );
}
#[test]
fn strict_version_zero_and_nameless_skip_keep_source_contract() {
    let (mut s, mut n) = seed();
    for (version, name, expected) in [
        (json!(101), "ignored", "Before"),
        (json!("101"), "string", "string"),
        (json!(0), "zero", "zero"),
        (json!(0), "zero again", "zero again"),
    ] {
        let t = enqueue(
            &mut s,
            &n,
            op(json!([{"id":"S1","name":name,"date_update":version}])),
        );
        (s, n) = finish(&s, &n, &t).into_parts();
        assert_eq!(
            s.saved.as_ref().unwrap().rows[0].effective["name"],
            expected
        );
    }
    let t = enqueue(
        &mut s,
        &n,
        op(json!([{"id":"S1","description":"not applied","date_update":222}])),
    );
    let c = finish(&s, &n, &t);
    assert_eq!(c.results()[0].effective["name"], "zero again");
    assert_ne!(c.results()[0].effective["date_update"], 222);
    let _ = c.into_parts();
}
#[test]
fn follower_does_not_retire_claimed_head_and_claims_are_exclusive() {
    let (mut s, n) = ready();
    let a = enqueue(&mut s, &n, op(json!([{"id":"S1","name":"First"}])));
    let pending = s.next_for(&a, "account", &owner(), 1, &n).unwrap().unwrap();
    let b = enqueue(&mut s, &n, op(json!([{"id":"S1","handle":"second"}])));
    assert!(pending.current(&s, "account", &owner(), 1, &n));
    assert!(
        s.next_for(&b, "account", &owner(), 1, &n)
            .unwrap()
            .is_none()
    );
    assert!(
        s.next_for(&a, "account", &owner(), 1, &n)
            .unwrap()
            .is_none()
    );
    drop(pending);
    let (s, n) = finish(&s, &n, &a).into_parts();
    let (s, n) = finish(&s, &n, &b).into_parts();
    assert_eq!(n["S1"].name, "First");
    assert_eq!(n["S1"].handle, "second");
    assert!(!s.has_pending());
}
#[tokio::test]
async fn exact_ticket_notification_has_no_lost_completion_and_cancel_poison() {
    let (mut s, n) = ready();
    let a = enqueue(&mut s, &n, op(json!([{"id":"S1","name":"First"}])));
    let mut b = enqueue(&mut s, &n, op(json!([{"id":"S2","name":"Second"}])));
    let (s, n) = finish(&s, &n, &a).into_parts();
    b.wait().await.unwrap();
    assert!(
        s.next_for(&b, "account", &owner(), 1, &n)
            .unwrap()
            .is_some()
    );
    drop(b);
    assert!(s.observe("account", &owner(), 1, &n).is_err());
    assert!(s.save().is_null());
}
#[test]
fn staged_commit_does_not_mark_ticket_finished_before_install() {
    let (mut s, n) = ready();
    let t = enqueue(&mut s, &n, op(json!([{"id":"S1","name":"First"}])));
    let c = finish(&s, &n, &t);
    assert!(!t.finished(&s));
    assert!(!t.completed.load(Ordering::Acquire));
    drop(c);
    drop(t);
    assert!(s.snapshot("account", &owner(), 1, &n).is_err());
}
#[test]
fn owner_epoch_token_and_untracked_native_changes_refuse() {
    let (mut s, n) = seed();
    let t = enqueue(&mut s, &n, op(json!([{"id":"S1","name":"Later"}])));
    let p = s.next_for(&t, "account", &owner(), 1, &n).unwrap().unwrap();
    let mut changed = owner();
    changed.token = "replacement".into();
    assert!(!p.current(&s, "account", &changed, 1, &n));
    assert!(!p.current(&s, "other", &owner(), 1, &n));
    assert!(!p.current(&s, "account", &owner(), 2, &n));
    let mut native = n.clone();
    native.get_mut("S1").unwrap().users.push("U_NEW".into());
    assert!(!p.current(&s, "account", &owner(), 1, &native));
}
#[test]
fn renderer_reply_requires_exact_fields_generation_and_observed_locale() {
    let (mut s, n) = ready();
    let t = enqueue(&mut s, &n, op(json!([{"id":"S1","name":"  I  "}])));
    let p = s.next_for(&t, "account", &owner(), 1, &n).unwrap().unwrap();
    assert_eq!(p.fields(), ["  I  "]);
    let r = RendererReply {
        generation: "test".into(),
        fields: p.fields().to_vec(),
        lowered: vec!["  ı  ".into()],
        locale: None,
    };
    assert!(s.commit(p, r, "account", &owner(), 1, &n).is_err());
    let p = s.next_for(&t, "account", &owner(), 1, &n).unwrap().unwrap();
    let r = RendererReply {
        generation: "test".into(),
        fields: p.fields().to_vec(),
        lowered: vec!["  ı  ".into()],
        locale: Some("tr".into()),
    };
    let (s, n) = s
        .commit(p, r, "account", &owner(), 1, &n)
        .unwrap()
        .into_parts();
    assert_eq!(s.saved.unwrap().rows[0].effective["_name_lc"], "  ı  ");
    assert_eq!(n["S1"].name, "  I  ");
}
#[test]
fn save_restore_requires_exact_pair_rebind_and_never_saves_auth() {
    let (s, n) = seed();
    let value = s.save();
    assert!(!value.to_string().contains(&owner().token));
    let restored = Store::restore(Some(&value), &n);
    assert!(restored.snapshot("account", &owner(), 1, &n).is_err());
    let lease = restored.begin_boot(&n).unwrap();
    let (s, n) = restored
        .observe_boot(lease, "account", &owner(), 2, &n, &boot())
        .unwrap()
        .into_parts();
    assert!(s.snapshot("account", &owner(), 2, &n).is_ok());
    let mut native = n;
    native.get_mut("S1").unwrap().handle = "untracked".into();
    assert!(Store::restore(Some(&value), &native).is_legacy());
    assert!(Store::restore(None, &native).is_legacy());
}
#[test]
fn native_membership_is_independent_from_canonical_and_absent_self_add_hydrates() {
    let (mut s, mut n) = ready();
    let t = enqueue(
        &mut s,
        &n,
        Operation::Metadata {
            origin: Origin::Upsert,
            rows: vec![json!({"id":"S1","name":"Crew","users":["U_SOURCE"]})],
            native_members: NativeMembers::Replace(vec!["U_NATIVE".into()]),
        },
    );
    (s, n) = finish(&s, &n, &t).into_parts();
    for id in ["S1", "S2"] {
        let t = enqueue(
            &mut s,
            &n,
            Operation::Event {
                kind: "subteam_self_added".into(),
                raw: json!({"subteam_id":id}),
                sonic: true,
            },
        );
        (s, n) = finish(&s, &n, &t).into_parts();
    }
    assert_eq!(n["S1"].users, ["U_NATIVE", "U_SELF"]);
    assert_eq!(n["S2"].users, ["U_SELF"]);
    assert!(n["S2"].label_source.is_none());
    assert_eq!(
        s.saved.as_ref().unwrap().rows[0].effective["users"],
        json!(["U_SOURCE", "U_SELF"])
    );
}
#[test]
fn members_list_replace_then_append_preserves_duplicates_and_count() {
    let (mut s, mut n) = seed();
    for (append, users) in [
        (false, vec!["U_A".into()]),
        (true, vec!["U_A".into(), "U_B".into()]),
    ] {
        let t = enqueue(
            &mut s,
            &n,
            Operation::Members {
                id: "S1".into(),
                users,
                append,
                skip_org_count: false,
            },
        );
        (s, n) = finish(&s, &n, &t).into_parts();
    }
    let row = &s.saved.as_ref().unwrap().rows[0].effective;
    assert_eq!(row["users"], json!(["U_A", "U_A", "U_B"]));
    assert_eq!(row["user_count"], 1);
    assert_eq!(row["hasFetchedMembers"], true);
    assert!(n["S1"].members_loaded);
}

/// Exact retained-module replay, generated by source-store/oracle.mjs. These
/// preimages are test-only Redux fixtures, never production history inference.
#[test]
fn original_module_oracle_batch_socket_and_list_projection() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    fn fixture(previous: &Value, user: &str) -> (Saved, Native) {
        let rows: Vec<_> = previous
            .as_object()
            .unwrap()
            .values()
            .map(|v| Current {
                source: v.clone(),
                effective: v.clone(),
            })
            .collect();
        let native: Native = rows
            .iter()
            .map(|r| {
                let g = UserGroup::from_observed(&r.effective).unwrap();
                (g.id.clone(), g)
            })
            .collect();
        (
            Saved {
                version: 1,
                account: "account".into(),
                team: "T_GROUP_TEST".into(),
                user: user.into(),
                rows,
                membership: None,
                membership_flags: BTreeMap::new(),
                native: native_proof(&native).unwrap(),
                pending: vec![],
            },
            native,
        )
    }
    fn run(case: &Value, previous: &Value, user: &str, operation: Operation) {
        let (mut saved, mut native) = fixture(previous, user);
        let fields = projection::fields(&saved.rows, &operation).unwrap();
        let lower = fields
            .into_iter()
            .map(|s| {
                let lower = s.to_lowercase();
                (s, lower)
            })
            .collect();
        projection::apply(&mut saved, &mut native, &operation, &lower).unwrap();
        let actual: serde_json::Map<String, Value> = saved
            .rows
            .iter()
            .map(|r| {
                (
                    r.effective["id"].as_str().unwrap().into(),
                    r.effective.clone(),
                )
            })
            .collect();
        assert_eq!(Value::Object(actual), case["store"], "{}", case["name"]);
        if let Some(order) = case["order"].as_array() {
            assert_eq!(
                saved
                    .rows
                    .iter()
                    .map(|r| r.effective["id"].clone())
                    .collect::<Vec<_>>(),
                *order
            );
        }
    }
    for case in oracle["cases"].as_array().unwrap() {
        run(
            case,
            &case["previous"],
            "U_SELF",
            Operation::Metadata {
                origin: if case["kind"] == "conditional" {
                    Origin::Fetch
                } else {
                    Origin::Upsert
                },
                rows: case["incoming"].as_array().unwrap().clone(),
                native_members: NativeMembers::Preserve,
            },
        );
    }
    for case in oracle["socketCases"].as_array().unwrap() {
        let kind = match case["name"].as_str().unwrap() {
            "subteamUpdated" => "subteam_updated",
            "subteamCreated" => "subteam_created",
            "subteamMembersChanged" => "subteam_members_changed",
            "subteamSelfAdded" => "subteam_self_added",
            "subteamSelfRemoved" => "subteam_self_removed",
            _ => panic!("uncovered source event"),
        };
        run(
            case,
            &case["previous"],
            case["self"].as_str().unwrap(),
            Operation::Event {
                kind: kind.into(),
                raw: case["event"].clone(),
                sonic: case["isSonic"].as_bool().unwrap(),
            },
        );
    }
    for case in oracle["listCases"].as_array().unwrap() {
        if case["requests"].as_array().unwrap().is_empty() {
            continue;
        }
        let id = case["previous"]["id"].as_str().unwrap();
        run(
            case,
            &json!({id:case["previous"].clone()}),
            "U_SELF",
            Operation::Members {
                id: id.into(),
                users: projection::strings(&case["reply"]["users"]).unwrap(),
                append: !case["cursor"].is_null(),
                skip_org_count: false,
            },
        );
    }
}
