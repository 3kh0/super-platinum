use super::*;
const FILE: &str = "FTITLE0001";
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some("file-choice-fixture".into());
    shell.core.active_team = Some("T1".into());
    shell.self_account.user_id = shell.core.workspaces["T1"].self_user_id.clone();
    shell
}
fn row(mode: &str) -> Value {
    json!({"id":FILE,"mode":mode,"filetype":mode,"created":101,
        "title":"Cafe\u{301} &amp; Plan","permalink":"https://fixture.invalid/docs/exact?x=1&y=2",
        "opaque":{"keep":[1,2]},"url_private":"https://fixture.invalid/not-the-href"})
}
fn title(raw: &Value) -> (file_title_source::Store, file_title_source::Observation) {
    let mut store = file_title_source::Store::new();
    store.ingest(raw).unwrap();
    let observed = store.row(FILE).unwrap();
    (store, observed)
}
fn project(
    shell: &ShellState,
    observed: file_title_source::Observation,
) -> Result<Projection, Error> {
    Projection::capture(
        shell,
        "T1",
        &shell.self_account.user_id,
        &json!({"ok":true,"workspaces":[{"id":"T1","name":"Actual team"}]}),
        observed,
        FILE,
    )
}

#[test]
fn whole_canvas_and_list_wire_uses_exact_permalink_not_label_or_private_url() {
    let shell = shell();
    for (mode, token_type) in [("quip", "canvas"), ("list", "file")] {
        let raw = row(mode);
        let (_, observed) = title(&raw);
        let projected = project(&shell, observed).unwrap();
        assert_eq!(projected.label, "Cafe\u{301} & Plan");
        assert_eq!(
            projected.value,
            json!({"type":token_type,"file_id":FILE,
            "text":raw["permalink"],"url":raw["permalink"]})
        );
        assert_eq!(projected._title.source(), &raw);
        assert!(projected.value.get("opaque").is_none());
        assert!(projected.value.get("style").is_none());
        assert!(projected.quip.is_none());
    }
}

#[test]
fn absent_null_or_empty_href_exports_id_without_derived_url() {
    let shell = shell();
    for href in [None, Some(Value::Null), Some(json!(""))] {
        let mut raw = row("list");
        raw.as_object_mut().unwrap().remove("permalink");
        if let Some(href) = href {
            raw["permalink"] = href;
        }
        let (_, observed) = title(&raw);
        let projected = project(&shell, observed).unwrap();
        assert_eq!(
            projected.value,
            json!({"type":"file","file_id":FILE,"text":FILE})
        );
    }
}

#[test]
fn actual_team_row_is_required_not_merely_active_team_string() {
    for raw in [
        json!({}),
        json!({"ok":true,"workspaces":[]}),
        json!({"ok":true,"workspaces":[{"id":"TOTHER"}]}),
        json!({"ok":true,"workspaces":["T1"]}),
        json!({"ok":true,"workspaces":[{"id":"T1"},{"id":"T1"}]}),
        json!({"ok":true,"error":"failed","workspaces":[{"id":"T1"}]}),
    ] {
        assert!(observed_team(&raw, "T1").is_err());
    }
    assert!(
        observed_team(
            &json!({"ok":true,"workspaces":[{"id":"T1","opaque":true}]}),
            "T1"
        )
        .is_ok()
    );
}

#[test]
fn canvas_blocks_require_independent_runtime_and_preserve_literal_text() {
    let mut shell = shell();
    let mut raw = row("quip");
    raw["title_blocks"] = json!([{"type":"rich_text","elements":[{"type":"rich_text_section",
        "elements":[{"type":"text","text":":wave: &amp; *literal*"}]}]}]);
    let (_, observed) = title(&raw);
    let projected = project(&shell, observed.clone()).unwrap();
    assert_eq!(projected.label, "&amp; *literal*");
    assert_eq!(
        projected.quip_proof(&shell),
        json!({"fileId":FILE,"current":true,"value":"nullish"})
    );
    assert!(projected.current_quip(&shell));
    shell.profile_generation += 1;
    assert!(projected.current_quip(&shell));
    shell.canvas_titles = quip_titles::State::fresh();
    assert_eq!(
        projected.quip_proof(&shell),
        json!({"fileId":FILE,"current":false,"value":"nullish"})
    );
    assert!(!projected.current_quip(&shell));
    shell.canvas_titles = quip_titles::State::default();
    assert!(project(&shell, observed).is_err());
}

#[test]
fn unsupported_semantic_blocks_refuse_instead_of_falling_back_to_plain_title() {
    let shell = shell();
    let mut raw = row("quip");
    raw["title_blocks"] = json!([{"type":"rich_text","elements":[{"type":"rich_text_section",
        "elements":[{"type":"user","user_id":"U12345678"}]}]}]);
    let (_, observed) = title(&raw);
    assert!(project(&shell, observed).is_err());
}

#[test]
fn exact_title_identity_and_source_receipt_cannot_be_replaced_silently() {
    let shell = shell();
    let raw = row("list");
    let (mut store, observed) = title(&raw);
    assert!(
        Projection::capture(
            &shell,
            "T1",
            &shell.self_account.user_id,
            &json!({"ok":true,"workspaces":[{"id":"T1"}]}),
            observed.clone(),
            "FOTHER0001"
        )
        .is_err()
    );
    let projected = project(&shell, observed).unwrap();
    assert!(projected._title.current(&store));
    let mut changed = raw.clone();
    changed["title"] = json!("Changed");
    store.ingest(&changed).unwrap();
    assert!(!projected._title.current(&store));
    assert_eq!(projected.label, "Cafe\u{301} & Plan");
    // Unsupported accepted update poisons title authority, retaining the raw
    // receipt; it cannot become a private-url or old-label fallback.
    changed["is_external"] = json!(true);
    changed["external_type"] = json!("app");
    assert!(store.ingest(&changed).is_err());
    assert!(store.row(FILE).is_err());
}

#[test]
fn unsupported_record_poll_remote_unknown_and_generic_file_shapes_refuse() {
    for raw in [
        json!({"mode":"list","subtype":"poll"}),
        json!({"mode":"hosted","filetype":"pdf"}),
        json!({"mode":"list","is_external":true,"external_type":"app"}),
        json!({"mode":"quip","isUnknown":true}),
        json!({"mode":"quip","isNonExistent":true}),
        json!({"mode":"quip","is_skill":true}),
        json!({"listId":FILE,"id":"Rec1234"}),
    ] {
        assert!(kind(&raw).is_err());
    }
}

#[test]
fn profile_context_admits_empty_canvas_without_granting_generic_or_list_fallback() {
    let shell = shell();
    let mut raw = row("quip");
    raw["title"] = json!(" \t");
    raw["title_blocks"] = json!([]);
    let (_, observed) = title(&raw);
    assert!(project(&shell, observed.clone()).is_err());
    assert!(CanonicalTitle::capture(&shell, &observed, FILE).is_err());
    let projected = Projection::capture_with_context(
        &shell,
        "T1",
        &shell.self_account.user_id,
        &json!({"ok":true,"workspaces":[{"id":"T1"}]}),
        observed,
        FILE,
        None,
        Some(super::super::super::title_locale::CanvasTitle::english()),
    )
    .unwrap();
    assert_eq!(projected.label, "Untitled");
    assert_eq!(projected.quip_proof(&shell), Value::Null);
    assert_eq!(
        projected.value,
        json!({"type":"canvas","file_id":FILE,
        "url":raw["permalink"],"text":raw["permalink"]})
    );
    assert_eq!(projected._title.source(), &raw);
    assert_eq!(
        projected.canvas_locale.unwrap().descriptor()["origin"],
        "application-presentation"
    );
    let mut list = row("list");
    list["title"] = json!("");
    let (_, observed) = title(&list);
    assert!(
        Projection::capture_with_context(
            &shell,
            "T1",
            &shell.self_account.user_id,
            &json!({"ok":true,"workspaces":[{"id":"T1"}]}),
            observed,
            FILE,
            None,
            Some(super::super::super::title_locale::CanvasTitle::english())
        )
        .is_err()
    );
}
