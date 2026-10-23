use super::*;
use crate::state::cfm_payload::{Effect, Kind, PlanInfoCache};
use serde_json::{Value, json};
const ACCOUNT: &str = "67c1fd64-2911-4d9a-aa50-fd73155a98f8";
fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TPLAN".into(),
        user_id: "UPLAN".into(),
        enterprise_id: None,
        name: "Plan cache".into(),
        url: "https://example.invalid".into(),
        token: "synthetic".into(),
    }
}
fn original() -> Workspace {
    let s = session();
    let mut w = Workspace::from_session(&s);
    w.cfm_payload
        .apply(&Effect {
            kind: Kind::PlanInfo,
            payload: json!({"is_plan_frozen":true,
        "reconnect_grace_period_start_ts":42,"opaque":[false,null]}),
        })
        .unwrap();
    w.cfm_payload.plan_info_cache =
        PlanInfoCache::accepted(ACCOUNT, &s, w.cfm_payload.plan_info.clone().unwrap());
    w.last_active_channel = Some("CPLAN".into());
    w
}
#[test]
fn plan_info_private_sqlite_reload_and_metadata_keep_history_without_live_state() {
    let path = std::env::temp_dir().join(format!("plan-info-{}.sqlite", uuid::Uuid::new_v4()));
    let w = original();
    {
        let c = Cache::open(&path).unwrap();
        c.save_workspace(&w).unwrap();
        WorkspaceSnapshot::metadata(&w).save(&c).unwrap();
    }
    {
        let c = Cache::open(&path).unwrap();
        for loaded in [
            c.load_workspace(&session()).unwrap().unwrap(),
            c.load_workspace_for_startup(&session()).unwrap().unwrap(),
        ] {
            assert_eq!(loaded.cfm_payload.plan_info, None);
            assert_eq!(
                loaded.cfm_payload.plan_info_cache,
                w.cfm_payload.plan_info_cache
            );
            assert!(
                loaded
                    .cfm_payload
                    .plan_info_cache
                    .as_ref()
                    .unwrap()
                    .matches(ACCOUNT, &session())
            );
            assert_eq!(loaded.last_active_channel, w.last_active_channel);
        }
    }
    std::fs::remove_file(path).unwrap();
}
#[test]
fn plan_info_bad_or_legacy_envelope_preserves_other_cached_fields() {
    let c = Cache::open(":memory:").unwrap();
    let w = original();
    c.save_workspace(&w).unwrap();
    let raw: String = c
        .conn
        .query_row(
            "select sidebar from workspaces where team_id='TPLAN'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for bad in [
        None,
        Some(Value::Null),
        Some(json!({})),
        Some(json!({"version":99})),
        Some(json!([])),
    ] {
        let mut side: Value = serde_json::from_str(&raw).unwrap();
        if let Some(v) = bad {
            side["plan_info"] = v;
        } else {
            side.as_object_mut().unwrap().remove("plan_info");
        }
        c.conn
            .execute(
                "update workspaces set sidebar=?1 where team_id='TPLAN'",
                [side.to_string()],
            )
            .unwrap();
        let loaded = c.load_workspace(&session()).unwrap().unwrap();
        assert!(loaded.cfm_payload.plan_info_cache.is_none());
        assert_eq!(loaded.last_active_channel, w.last_active_channel);
    }
}
#[test]
fn plan_info_identity_never_follows_account_user_url_or_enterprise_replacement() {
    let r = original().cfm_payload.plan_info_cache.unwrap();
    let s = session();
    assert!(!r.matches("c386ef8b-2908-4c2d-817d-9dbb1394866d", &s));
    for n in 0..4 {
        let mut other = s.clone();
        match n {
            0 => other.team_id = "OTHER".into(),
            1 => other.user_id = "OTHER".into(),
            2 => other.url = "https://else.invalid".into(),
            _ => other.enterprise_id = Some("EOTHER".into()),
        };
        assert!(!r.matches(ACCOUNT, &other));
        assert!(PlanInfoCache::restore(&serde_json::to_value(&r).unwrap(), &other).is_none());
    }
    assert!(PlanInfoCache::accepted("not-a-uuid", &s, json!({})).is_none());
}
#[test]
fn plan_info_restored_merge_keeps_empty_and_replaces_explicit_null() {
    let w = original();
    let retained = w.cfm_payload.plan_info_cache.unwrap();
    let mut stores = crate::state::cfm_payload::Stores::default();
    stores.plan_info = Some(retained.raw().clone());
    stores
        .apply(&Effect {
            kind: Kind::PlanInfo,
            payload: json!({}),
        })
        .unwrap();
    assert_eq!(stores.plan_info.as_ref(), Some(retained.raw()));
    stores
        .apply(&Effect {
            kind: Kind::PlanInfo,
            payload: json!({"reconnect_grace_period_start_ts":null}),
        })
        .unwrap();
    assert!(stores.plan_info.as_ref().unwrap()["reconnect_grace_period_start_ts"].is_null());
    let before = stores.clone();
    assert!(
        stores
            .apply(&Effect {
                kind: Kind::PlanInfo,
                payload: json!([])
            })
            .is_err()
    );
    assert_eq!(before, stores);
}
#[test]
fn plan_info_unmarked_raw_and_failed_write_do_not_create_or_erase_history() {
    let c = Cache::open(":memory:").unwrap();
    let mut w = original();
    c.save_workspace(&w).unwrap();
    let before = w.clone();
    c.conn.pragma_update(None, "query_only", true).unwrap();
    w.cfm_payload.plan_info_cache = None;
    assert!(c.save_workspace(&w).is_err());
    assert_eq!(
        c.load_workspace(&session())
            .unwrap()
            .unwrap()
            .cfm_payload
            .plan_info_cache,
        before.cfm_payload.plan_info_cache
    );
    c.conn.pragma_update(None, "query_only", false).unwrap();
    c.save_workspace(&w).unwrap();
    assert!(
        c.load_workspace(&session())
            .unwrap()
            .unwrap()
            .cfm_payload
            .plan_info_cache
            .is_none()
    );
}
