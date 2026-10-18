use super::*;
use crate::state::directory_usergroups::{NativeMembers, Operation, Origin, RendererReply};
use serde_json::json;

fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://example.invalid".into(),
        token: "synthetic-token".into(),
    }
}
fn bind(w: &mut Workspace, owner: &WorkspaceSession) {
    let lease = w.directory_usergroups.begin_boot(&w.usergroups).unwrap();
    let commit = w
        .directory_usergroups
        .observe_boot(
            lease,
            "fixture-account",
            owner,
            1,
            &w.usergroups,
            &json!({"ok":true,"team":{"id":"T1"},"self":{"id":"U1"}}),
        )
        .unwrap();
    (w.directory_usergroups, w.usergroups) = commit.into_parts();
}
fn metadata(w: &mut Workspace, owner: &WorkspaceSession) {
    let ticket = w
        .directory_usergroups
        .enqueue(
            "fixture-account",
            owner,
            1,
            "read",
            &w.usergroups,
            Operation::Metadata {
                origin: Origin::Fetch,
                native_members: NativeMembers::Preserve,
                rows: vec![
                    json!({"id":"S2","name":"Beta","handle":"beta","opaque":2}),
                    json!({"id":"S1","name":"Alpha","handle":"alpha","opaque":1}),
                ],
            },
        )
        .unwrap();
    let pending = w
        .directory_usergroups
        .next_for(&ticket, "fixture-account", owner, 1, &w.usergroups)
        .unwrap()
        .unwrap();
    let reply = RendererReply {
        generation: pending.generation().into(),
        fields: pending.fields().to_vec(),
        lowered: pending
            .fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
        locale: Some("en-US".into()),
    };
    let commit = w
        .directory_usergroups
        .commit(pending, reply, "fixture-account", owner, 1, &w.usergroups)
        .unwrap();
    (w.directory_usergroups, w.usergroups) = commit.into_parts();
    drop(ticket);
}

#[test]
fn group_history_survives_full_startup_and_metadata_cache_with_order_and_no_runtime_authority() {
    let cache = Cache::open(":memory:").unwrap();
    let owner = session();
    let mut w = Workspace::from_session(&owner);
    bind(&mut w, &owner);
    metadata(&mut w, &owner);
    cache.save_workspace(&w).unwrap();
    WorkspaceSnapshot::metadata(&w).save(&cache).unwrap();
    for mut loaded in [
        cache.load_workspace(&owner).unwrap().unwrap(),
        cache.load_workspace_for_startup(&owner).unwrap().unwrap(),
    ] {
        assert!(
            loaded
                .directory_usergroups
                .snapshot("fixture-account", &owner, 1, &loaded.usergroups)
                .is_err()
        );
        assert!(
            !loaded
                .directory_usergroups
                .save()
                .to_string()
                .contains("synthetic-token")
        );
        bind(&mut loaded, &owner);
        let snapshot = loaded
            .directory_usergroups
            .snapshot("fixture-account", &owner, 1, &loaded.usergroups)
            .unwrap();
        assert_eq!(
            snapshot
                .rows()
                .iter()
                .map(|row| row.effective["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["S2", "S1"]
        );
        assert_eq!(snapshot.rows()[0].effective["opaque"], 2);
    }
}

#[test]
fn old_malformed_and_native_mismatched_cache_do_not_invent_history() {
    for mode in 0..3 {
        let cache = Cache::open(":memory:").unwrap();
        let owner = session();
        let mut w = Workspace::from_session(&owner);
        bind(&mut w, &owner);
        metadata(&mut w, &owner);
        cache.save_workspace(&w).unwrap();
        match mode {
            0 => {
                cache
                    .conn
                    .execute("update workspaces set sidebar = '{}'", [])
                    .unwrap();
            }
            1 => {
                cache
                    .conn
                    .execute(
                        "update workspaces set sidebar = ?1",
                        params![r#"{"directory_usergroups":{"version":999}}"#],
                    )
                    .unwrap();
            }
            _ => {
                cache
                    .conn
                    .execute(
                        "update usergroups set json = ?1 where group_id = 'S2'",
                        params![json!({"id":"S2","handle":"changed"}).to_string()],
                    )
                    .unwrap();
            }
        }
        let loaded = cache.load_workspace(&owner).unwrap().unwrap();
        assert!(loaded.directory_usergroups.is_legacy());
        assert!(
            loaded
                .directory_usergroups
                .begin_boot(&loaded.usergroups)
                .is_err()
        );
    }
}
