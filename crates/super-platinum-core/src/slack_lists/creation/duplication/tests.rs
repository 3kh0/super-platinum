use super::super::tests::fixture;
use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{
        api,
        client::{RequestBody, SlackClient},
    },
};

fn page() -> RecordsPage {
    RecordsPage::parse(json!({"ok":true,"records":[
        {"id":"R_SOURCE","position":"5000000000","fields":[{"key":"name","text":"Original"}]},
        {"id":"R_NEXT","position":"5000000001","fields":[]}
    ]}))
    .unwrap()
}
fn intent(document: &Document, page: &RecordsPage, policy: &PersonalTaskLists) -> CreationIntent {
    CreationIntent::capture_duplicate(
        document,
        page,
        "R_SOURCE",
        Some("full"),
        Some(policy),
        Some("V_ALL"),
    )
    .unwrap()
}

#[test]
fn fractional_keys_match_recovered_ommm_source_oracle() {
    // Evaluated from the unchanged Ommm function in recovered S, offset4545426.
    for (lower, upper, expected) in [
        (None, None, "5000000000"),
        (None, Some("5000000000"), "4999999999"),
        (None, Some("5000000000.V"), "5000000000"),
        (None, Some("0000000001.V"), "0000000000"),
        (Some("5000000000"), None, "5000000001"),
        (Some("5000000000.V"), None, "5000000001"),
        (Some("5000000000"), Some("5000000002"), "5000000001"),
        (Some("5000000000"), Some("5000000001"), "5000000000.V"),
        (Some("5000000000.V"), Some("5000000000.W"), "5000000000.VV"),
        (
            Some("5000000000.VVVV"),
            Some("5000000000.VVVW"),
            "5000000000.VVVVV",
        ),
        (Some("5000000000.z"), Some("5000000001"), "5000000000.zV"),
        (Some("5000000000"), Some("5000000000.1"), "5000000000.0V"),
        (Some("5000000000.0V"), Some("5000000000.1"), "5000000000.0l"),
        (Some("0000000000"), None, "0000000001"),
    ] {
        assert_eq!(position::between(lower, upper).unwrap(), expected);
    }
    for (lower, upper) in [
        (None, Some("0000000000")),
        (Some("9999999999"), None),
        (Some("100"), None),
        (Some("5000000000.V0"), None),
        (Some("5000000000.V"), Some("5000000000.V")),
    ] {
        assert!(position::between(lower, upper).is_err());
    }
    let pathological = format!("5000000000.{}", "V".repeat(2048));
    assert!(position::between(Some(&pathological), None).is_err());
}

#[test]
fn duplicate_uses_canonical_view_adjacency_and_truthy_fallback() {
    let (document, policy) = fixture();
    let mut rows = page();
    assert_eq!(intent(&document, &rows, &policy).position(), "5000000000.V");
    rows.records[0].raw["view_positions"] = json!({"v_all":"5000000002"});
    assert_eq!(intent(&document, &rows, &policy).position(), "5000000003");
    for fallback in [json!(0), json!(""), Value::Null] {
        rows.records[0].raw["view_positions"] = json!({"v_all":fallback});
        assert_eq!(intent(&document, &rows, &policy).position(), "5000000000.V");
    }
    rows.records[0].raw["view_positions"] = json!({"V_ALL":5000000002_u64});
    assert!(
        CreationIntent::capture_duplicate(
            &document,
            &rows,
            "R_SOURCE",
            Some("full"),
            Some(&policy),
            Some("V_ALL")
        )
        .is_err()
    );
    rows.records[0].raw["view_positions"] = json!({});
    for (index, number) in [5000000000_u64, 5000000001].into_iter().enumerate() {
        rows.records[index].raw["position"] = json!(number);
    }
    assert_eq!(intent(&document, &rows, &policy).position(), "5000000000.V");
    rows.records[0].raw["position"] = json!(100);
    assert!(
        CreationIntent::capture_duplicate(
            &document,
            &rows,
            "R_SOURCE",
            Some("full"),
            Some(&policy),
            Some("V_ALL")
        )
        .is_err()
    );
    for row in &mut rows.records {
        row.raw["position"] = Value::Null;
    }
    assert_eq!(intent(&document, &rows, &policy).position(), "");
}

#[test]
fn creation_date_ties_preserve_javascript_utf16_and_signed_zero_order() {
    let (document, policy) = fixture();
    for (source_date, peer_date, source_first, can_insert) in [
        (json!("\u{1f600}"), json!("\u{e000}"), true, false),
        (json!("\u{e000}"), json!("\u{1f600}"), true, true),
        (json!(0.0), json!(-0.0), true, false),
        (json!(-0.0), json!(0.0), true, false),
        (json!(0.0), json!(-0.0), false, true),
        (json!(-0.0), json!(0.0), false, true),
    ] {
        let source = json!({"id":"R_SOURCE","position":"5000000000",
            "date_created":source_date,"fields":[]});
        let peer = json!({"id":"R_PEER","position":"5000000000",
            "date_created":peer_date,"fields":[]});
        let records = if source_first {
            vec![source, peer]
        } else {
            vec![peer, source]
        };
        let mut rows = RecordsPage::parse(json!({"ok":true,"records":records})).unwrap();
        rows.records.push(
            Record::parse(json!({"id":"R_NEXT",
            "position":"5000000001","fields":[]}))
            .unwrap(),
        );
        let captured = CreationIntent::capture_duplicate(
            &document,
            &rows,
            "R_SOURCE",
            Some("full"),
            Some(&policy),
            Some("V_ALL"),
        );
        if can_insert {
            assert_eq!(captured.unwrap().position(), "5000000000.V");
        } else {
            // The source precedes a peer with the same position. Ommm rejects
            // equal boundaries; reordering a tie must never manufacture space.
            assert!(captured.is_err());
        }
    }
}

#[test]
fn duplicate_preflight_owns_source_and_neighbor_and_requires_complete_page() {
    let (document, policy) = fixture();
    let rows = page();
    let captured = intent(&document, &rows, &policy);
    assert!(
        captured
            .revalidate(&document, Some("full"), Some(&policy), Some("V_ALL"))
            .is_err()
    );
    assert!(
        captured
            .revalidate_with_records(&document, &rows, Some("full"), Some(&policy), Some("V_ALL"))
            .is_ok()
    );
    let mut incomplete = rows.clone();
    incomplete.next_cursor = Some("next".into());
    let mut changed = rows.clone();
    changed.records[0].raw["fields"][0]["text"] = json!("Changed");
    let mut neighbor = rows.clone();
    neighbor.records[1].raw["position"] = json!("5000000002");
    let mut removed = rows.clone();
    removed.records.remove(0);
    let mut archived = rows.clone();
    archived.records[0].raw["archived"] = json!(true);
    let mut deleted = rows.clone();
    deleted.records[0].raw["is_deleted"] = json!(true);
    let mut added = rows.clone();
    added.records.push(
        Record::parse(json!({"id":"R_UNRELATED","position":"5000000010","fields":[]})).unwrap(),
    );
    for rows in [
        incomplete, changed, neighbor, removed, archived, deleted, added,
    ] {
        assert!(
            captured
                .revalidate_with_records(
                    &document,
                    &rows,
                    Some("full"),
                    Some(&policy),
                    Some("V_ALL")
                )
                .is_err()
        );
    }
    // A neighbor's unrelated cell edit cannot change the insertion boundary.
    let mut unrelated = rows.clone();
    unrelated.records[1].raw["fields"] = json!([{"key":"name","text":"Elsewhere"}]);
    assert!(
        captured
            .revalidate_with_records(
                &document,
                &unrelated,
                Some("full"),
                Some(&policy),
                Some("V_ALL")
            )
            .is_ok()
    );
}

#[test]
fn duplicate_receipt_needs_new_identity_but_parent_and_fields_are_server_owned() {
    let (document, policy) = fixture();
    let captured = intent(&document, &page(), &policy);
    let response = json!({"ok":true,"record":{"id":"R_COPY","parent_record_id":"R_PARENT","fields":[{"key":"name","text":"Server value"}]}});
    let receipt = captured.validate_response(&response).unwrap();
    let readback = RecordsPage::parse(json!({"ok":true,"records":[receipt.raw]})).unwrap();
    assert!(
        captured
            .readback_record(&document, &readback, &receipt)
            .is_some()
    );
    for id in ["R_SOURCE", "R_NEXT"] {
        let mut wrong = response.clone();
        wrong["record"]["id"] = json!(id);
        assert!(captured.validate_response(&wrong).is_err());
    }
    for parent in [json!(123), json!(" R_PARENT"), json!("R_COPY")] {
        let mut wrong = response.clone();
        wrong["record"]["parent_record_id"] = parent;
        assert!(captured.validate_response(&wrong).is_err());
    }
    let completed = CreationIntent::capture_duplicate(
        &document,
        &page(),
        "R_SOURCE",
        Some("full"),
        Some(&policy),
        Some("V_DONE"),
    )
    .unwrap();
    assert!(completed.validate_response(&response).is_err());
    let mut response = response;
    response["record"]["fields"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"done","checkbox":true}));
    assert!(completed.validate_response(&response).is_ok());
}

#[test]
fn duplicate_wire_forwards_source_and_adjacent_position_without_guessed_parent() {
    let (document, policy) = fixture();
    let intent = CreationIntent::capture_duplicate(
        &document,
        &page(),
        "R_SOURCE",
        Some("full"),
        Some(&policy),
        Some("V_DONE"),
    )
    .unwrap();
    let workspace = WorkspaceSession {
        team_id: "T_TEST".into(),
        enterprise_id: None,
        user_id: "U_TEST".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let timestamp = ListClock::new()
        .tick(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        )
        .unwrap();
    let request = api::list_record_create(&SlackClient::default(), &workspace, &intent, &timestamp);
    assert!(!request.retry_safe());
    assert!(request.url.contains("/api/lists.records.create?"));
    let RequestBody::Form(fields) = request.body else {
        panic!("form expected")
    };
    assert!(fields.contains(&("duplicated_record_id".into(), "R_SOURCE".into())));
    assert!(fields.contains(&("position".into(), "5000000000.V".into())));
    assert!(fields.contains(&(
        "initial_fields".into(),
        json!([{"column_id":"C_DONE","checkbox":true}]).to_string()
    )));
    assert!(!fields.iter().any(|(key, _)| matches!(
        key.as_str(),
        "parent_record_id" | "view_id" | "fields" | "client_id" | "id"
    )));
}
