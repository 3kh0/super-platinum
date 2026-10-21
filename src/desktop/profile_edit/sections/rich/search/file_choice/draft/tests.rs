use super::*;

fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    s.core.active_account = Some("display-title-fixture".into());
    s.core.active_team = Some("T1".into());
    s.self_account.user_id = s.core.workspaces["T1"].self_user_id.clone();
    s
}
fn title(mode: &str) -> file_title_source::Observation {
    let mut store = file_title_source::Store::new();
    store
        .ingest(&json!({"id":"FDISPLAY1","mode":mode,"filetype":mode,
        "created":101,"title":"Actual &amp; Title"}))
        .unwrap();
    store.row("FDISPLAY1").unwrap()
}
fn label(s: &ShellState, token: &Value, title: file_title_source::Observation) -> Option<String> {
    token_label(s, "T1", &s.self_account.user_id, token, title)
}
#[test]
fn exact_occurrence_keys_keep_styles_opaque_fields_and_raw_draft() {
    let s = shell();
    let a = json!({"type":"file","file_id":"FDISPLAY1","text":"forged title",
        "url":"https://fixture.invalid/wire","style":{"bold":true},"opaque":{"order":1}});
    let b = json!({"type":"file","file_id":"FDISPLAY1","text":"different wire",
        "style":{"italic":true},"opaque":{"order":2}});
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section",
        "elements":[a.clone(),b.clone(),a.clone()]}]}),
    ];
    let before = blocks.clone();
    let labels = collect(&blocks, |token| label(&s, token, title("list")));
    assert_eq!(labels.len(), 2);
    assert_eq!(
        labels[&serde_json::to_string(&a).unwrap()],
        "Actual & Title"
    );
    assert_eq!(
        labels[&serde_json::to_string(&b).unwrap()],
        "Actual & Title"
    );
    assert_eq!(blocks, before);
}
#[test]
fn only_matching_whole_tokens_receive_canonical_labels() {
    let s = shell();
    for token in [
        json!({"type":"file","file_id":"FDISPLAY1"}),
        json!({"type":"canvas","file_id":"OTHER"}),
        json!({"type":"canvas","file_id":"FDISPLAY1","section_id":"section"}),
        json!({"type":"canvas","file_id":"FDISPLAY1","is_skill_invocation":true}),
        json!({"type":"canvas","file_id":"FDISPLAY1","style":null}),
    ] {
        assert!(label(&s, &token, title("quip")).is_none());
    }
    let token = json!({"type":"file","file_id":"FDISPLAY1","record_id":"opaque",
        "section_id":"ignored file metadata","style":{"bold":true}});
    assert_eq!(
        label(&s, &token, title("list")).as_deref(),
        Some("Actual & Title")
    );
}
#[test]
fn block_title_requires_independent_quip_runtime_and_retires_with_it() {
    let mut s = shell();
    let mut store = file_title_source::Store::new();
    store.ingest(&json!({"id":"FDISPLAY1","mode":"quip","filetype":"quip","created":101,
        "title":"not used","title_blocks":[{"type":"rich_text","elements":[
        {"type":"rich_text_section","elements":[{"type":"text","text":":wave: Literal &amp;"}]}]}]})).unwrap();
    let observed = store.row("FDISPLAY1").unwrap();
    let captured = CanonicalTitle::capture(&s, &observed, "FDISPLAY1").unwrap();
    assert_eq!(captured.label, "Literal &amp;");
    assert!(captured.current_quip(&s));
    s.canvas_titles = quip_titles::State::fresh();
    assert!(!captured.current_quip(&s));
    s.canvas_titles = quip_titles::State::default();
    let token = json!({"type":"canvas","file_id":"FDISPLAY1","text":"forged fallback"});
    assert!(label(&s, &token, observed).is_none());
}
#[test]
fn no_current_source_never_promotes_token_or_typed_file_title() {
    let s = shell();
    let typed = super_platinum_core::slack::models::File {
        id: Some("FDISPLAY1".into()),
        title: Some("Typed-only title".into()),
        filetype: Some("quip".into()),
        ..Default::default()
    };
    let blocks = vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section",
        "elements":[{"type":"canvas","file_id":"FDISPLAY1","text":"Wire-only title",
        "title":"Raw-only title","file":typed}]}]}),
    ];
    assert!(draft_file_labels(&s, "T1", &blocks).is_empty());
    assert!(draft_file_labels(&s, "OTHER", &blocks).is_empty());
}
