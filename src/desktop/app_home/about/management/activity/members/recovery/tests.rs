use super::*;
use serde_json::json;

fn empty() -> Acquisition {
    Acquisition {
        requested: vec!["U00000001".into(), "U00000002".into()],
        rows: vec![],
        failed: vec![],
        disconnected: vec![],
        interactions: BTreeMap::new(),
        rounds: vec![],
    }
}
fn chunk(ids: &[&str], raw: Value) -> Chunk {
    let requested = ids.iter().map(|v| v.to_string()).collect::<Vec<_>>();
    let read = Read::capture(requested.clone()).unwrap();
    Chunk {
        requested,
        pages: vec![Page::decode(&read, raw, None).unwrap()],
        error: None,
        terminal: true,
    }
}

#[test]
fn source_chunk_order_and_final_interaction_override_precede_any_projection() {
    let mut a = empty();
    let pending = a.append_round(vec![chunk(
        &["U00000001", "U00000002"],
        json!({"ok":true,
        "results":[{"id":"U00000002"},{"id":"U00000001"}],
        "pending_ids":["U00000001"],"can_interact":{"U00000001":true}}),
    )]);
    assert_eq!(pending, vec!["U00000001"]);
    assert_eq!(
        a.rows
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["U00000002", "U00000001"]
    );
    assert!(
        a.append_round(vec![chunk(
            &["U00000001"],
            json!({"ok":true,"results":[],"can_interact":{"U00000001":false}})
        )])
        .is_empty()
    );
    assert_eq!(a.interactions.get("U00000001"), Some(&false));
    assert_eq!(a.rounds.len(), 2);
}

#[test]
fn failed_chunk_keeps_earlier_receipt_without_publishing_its_partial_rows() {
    let mut a = empty();
    let mut failed = chunk(
        &["U00000001"],
        json!({"ok":true,"results":[{"id":"U00000001"}],"can_interact":{"U00000001":true}}),
    );
    failed.error = Some(Error::Api("synthetic_later_page_failure".into()));
    a.append_round(vec![
        failed,
        chunk(
            &["U00000002"],
            json!({"ok":true,"results":[{"id":"U00000002"}]}),
        ),
    ]);
    assert_eq!(a.failed, vec!["U00000001"]);
    assert_eq!(a.rows.len(), 1);
    assert_eq!(a.rows[0]["id"], "U00000002");
    assert!(!a.interactions.contains_key("U00000001"));
    assert_eq!(a.rounds[0][0].pages.len(), 1);
    assert!(a.rounds[0][0].error.is_some());
}

#[test]
fn source_invalid_ids_and_raw_failed_ids_are_retained_distinctly() {
    let mut a = empty();
    a.append_round(vec![chunk(
        &["U00000001", "U00000002"],
        json!({"ok":true,
        "failed_ids":["U00000001"],"invalid_ids":["U00000002"],"disconnected_ids":["U00000001"]}),
    )]);
    assert_eq!(a.failed, vec!["U00000002"]);
    assert_eq!(
        a.rounds[0][0].pages[0].failed_ids(),
        &["U00000001".to_string()]
    );
    assert_eq!(a.disconnected, vec!["U00000001"]);
    assert!(a.rows.is_empty());
}

#[test]
fn local_refusal_cannot_become_unknown_manager_terminal_evidence() {
    let mut a = empty();
    let mut refused = chunk(&["U00000001"], json!({"ok":true,"results":[]}));
    refused.error = Some(unavailable());
    refused.terminal = false;
    a.append_round(vec![refused]);
    assert!(!a.terminal());
    assert_eq!(a.failed, vec!["U00000001"]);
    let mut server = empty();
    let mut rejected = chunk(&["U00000001"], json!({"ok":true,"results":[]}));
    rejected.error = Some(Error::HttpStatus {
        status: 500,
        retry_after_secs: None,
    });
    server.append_round(vec![rejected]);
    assert!(server.terminal());
}

#[test]
fn dispatch_rechecks_owner_cooldown_and_endpoint_expiry_after_a_hold() {
    let (mut shell, visit) = super::super::tests::setup();
    let query = Query::capture(&mut shell, visit).unwrap();
    let observed = Instant::now();
    let endpoint = DirectoryEndpoint::decode(
        &query.owner.ctx.session,
        &json!({"ok":true,"ttl_seconds":5,"url":format!(
            "https://edgeapi.slack.com/cache/{}", query.owner.ctx.session.team_id)}),
        observed,
    )
    .unwrap();
    assert!(dispatch_current(&shell, &query, &endpoint, observed));
    assert!(!dispatch_current(
        &shell,
        &query,
        &endpoint,
        observed + Duration::from_millis(2500)
    ));
    navigation::retain_limit(&mut shell, &query.owner.ctx, Some(60));
    assert!(query.current(&shell)); // already-issued responses keep ownership
    assert!(!dispatch_current(&shell, &query, &endpoint, observed));
}

#[test]
fn dispatch_refuses_same_text_replacement_and_complete_auth_rotation() {
    for change in 0..3 {
        let (mut shell, visit) = super::super::tests::setup();
        let query = Query::capture(&mut shell, visit).unwrap();
        let observed = Instant::now();
        let endpoint = DirectoryEndpoint::decode(
            &query.owner.ctx.session,
            &json!({"ok":true,"ttl_seconds":300,"url":format!(
                "https://edgeapi.slack.com/cache/{}", query.owner.ctx.session.team_id)}),
            observed,
        )
        .unwrap();
        match change {
            0 => edit(&mut shell, visit, query.member.raw().into()),
            1 => shell.core.active_account = Some("retired-account".into()),
            _ => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .d_cookie
                .push_str("-rotated"),
        }
        assert!(!dispatch_current(&shell, &query, &endpoint, observed));
    }
}
