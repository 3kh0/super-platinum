use super::*;
use serde_json::json;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::models::{Channel, User},
};

fn workspace() -> Workspace {
    let mut workspace = Workspace::from_session(&WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "SELF".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    });
    let user: User =
        serde_json::from_value(json!({"id":"U1","profile":{"display_name":"Maya"}})).unwrap();
    let channel: Channel = serde_json::from_value(json!({"id":"C1","name":"releases"})).unwrap();
    workspace.users.insert("U1".into(), user);
    workspace.channels.insert("C1".into(), channel);
    workspace
}
fn values(kind: &str, field: Value) -> (Column, Record) {
    (
        Column {
            id: "COLUMN".into(),
            key: "field".into(),
            name: "Field".into(),
            kind: kind.into(),
            raw: json!({"id":"COLUMN","key":"field","type":kind}),
        },
        Record::parse(json!({"id":"RECORD","fields":[field]})).unwrap(),
    )
}

#[test]
fn number_display_uses_schema_and_workspace_locale_without_changing_raw_value() {
    let media = MediaRegistry::default();
    let mut workspace = workspace();
    for kind in ["number", "currency"] {
        let (mut column, record) = values(kind, json!({"key":"field","number":[1234.5]}));
        let original = record.raw.clone();
        assert_eq!(
            project(Some(&workspace), &media, &column, &record).0,
            "1,235"
        );
        column.raw["options"] = json!({"precision":2});
        workspace.locale = Some("de-DE".into());
        assert_eq!(
            project(Some(&workspace), &media, &column, &record).0,
            "1.234,50"
        );
        assert_eq!(record.raw, original);
        assert_eq!(record.fields["field"]["number"], json!([1234.5]));
        workspace.locale = None;
    }
}

#[test]
fn structured_links_apply_existing_scheme_and_credentials_policy() {
    let media = MediaRegistry::default();
    for url in [
        "javascript:alert(1)",
        "data:text/html,unsafe",
        "https://name:password@example.test",
    ] {
        let (column, record) = values(
            "link",
            json!({"key":"field","link":[{"original_url":url,"display_name":"Safe label"}]}),
        );
        let (text, content) = project(None, &media, &column, &record);
        assert_eq!(text, "Safe label");
        assert_eq!(
            content,
            CellContent::Rich(vec![RichNode::Text("Safe label".into())])
        );
    }
    let (column, record) = values(
        "link",
        json!({"key":"field","link":[{"original_url":"https://example.test/path","display_name":"Notes"}]}),
    );
    let (_, content) = project(None, &media, &column, &record);
    assert!(
        matches!(content,CellContent::Rich(nodes) if matches!(&nodes[..],[RichNode::Link { url, .. }] if url == "https://example.test/path"))
    );
}

#[test]
fn references_resolve_only_existing_workspace_names_and_retain_unknown_ids() {
    let workspace = workspace();
    let media = MediaRegistry::default();
    let (column, record) = values(
        "user",
        json!({"key":"field","user":["U1","UNKNOWN"],"text":"Old fallback"}),
    );
    let (text, content) = project(Some(&workspace), &media, &column, &record);
    assert_eq!(text, "@Maya, UNKNOWN");
    assert!(
        matches!(content,CellContent::Rich(nodes) if matches!(&nodes[..],[RichNode::UserMention { .. },RichNode::Text(_),RichNode::Text(id)] if id == "UNKNOWN"))
    );
    assert_eq!(
        project(None, &media, &column, &record),
        ("Old fallback".into(), CellContent::Plain)
    );
    let (column, record) = values("channel", json!({"key":"field","channel":["C1"]}));
    assert_eq!(
        project(Some(&workspace), &media, &column, &record).0,
        "#releases"
    );
    let (column, record) = values(
        "attachment",
        json!({"key":"field","attachment":["F1"],"text":"Planning document"}),
    );
    assert_eq!(
        project(Some(&workspace), &media, &column, &record),
        ("Planning document".into(), CellContent::Plain)
    );
}

#[test]
fn rich_cells_keep_formatting_but_cannot_introduce_layout_actions_or_unsafe_links() {
    let workspace = workspace();
    let media = MediaRegistry::default();
    let (column, record) = values(
        "text",
        json!({"key":"field","rich_text":[
            {"type":"actions","elements":[{"type":"button","text":{"type":"plain_text","text":"Forbidden"},"url":"https://example.test"}]},
            {"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                {"type":"text","text":"Bold ","style":{"bold":true}},
                {"type":"link","text":"Literal","url":"javascript:alert(1)"},
                {"type":"button","text":{"type":"plain_text","text":"Forbidden nested"}},
                {"type":"image","image_url":"https://example.test/image.png","alt_text":"No embed"}
            ]}]}
        ]}),
    );
    let (text, content) = project(Some(&workspace), &media, &column, &record);
    assert_eq!(text, "Bold Literal");
    let CellContent::Rich(nodes) = content else {
        panic!()
    };
    assert!(
        matches!(&nodes[..],[RichNode::Paragraph(children)] if matches!(&children[..],[RichNode::StyledText {bold:true,..},RichNode::Text(label)] if label == "Literal"))
    );
}

#[test]
fn rich_projection_limits_breadth_text_and_depth_with_explicit_fallback() {
    let workspace = workspace();
    let media = MediaRegistry::default();
    let mut deep = json!({"type":"text","text":"end"});
    for _ in 0..MAX_RICH_DEPTH + 2 {
        deep = json!({"type":"rich_text_section","elements":[deep]});
    }
    for block in [
        json!({"type":"rich_text_section","elements":(0..MAX_RICH_NODES).map(|_|json!({"type":"text","text":"x"})).collect::<Vec<_>>()}),
        json!({"type":"rich_text_section","elements":[{"type":"link","text":"Short label","url":format!("https://example.test/{}", "x".repeat(MAX_RICH_BYTES))}]}),
        deep,
    ] {
        let (column, record) = values(
            "text",
            json!({"key":"field","rich_text":[block],"text":"Potentially incomplete fallback"}),
        );
        assert_eq!(
            project(Some(&workspace), &media, &column, &record),
            (TOO_LARGE.into(), CellContent::Plain)
        );
        assert_eq!(
            project(None, &media, &column, &record),
            (TOO_LARGE.into(), CellContent::Plain)
        );
    }
}

#[test]
fn rich_sanitizer_copies_only_consumed_scalars_and_keeps_plain_fallback() {
    let mut ignored = json!({"payload":"not rendered"});
    for _ in 0..100 {
        ignored = json!({"unknown":ignored});
    }
    let raw = json!({"type":"rich_text_section","untrusted":ignored,"elements":[{"type":"text","text":"Readable","untrusted":ignored,"style":{"bold":true,"unknown":ignored}}]});
    let mut budget = Budget::default();
    let clean = restricted_block(&raw, 0, &mut budget).unwrap();
    assert!(!budget.incomplete);
    assert_eq!(
        clean,
        json!({"type":"rich_text_section","elements":[{"type":"text","text":"Readable","style":{"bold":true}}]})
    );
    let (column, record) = values("rich_text", json!({"key":"field","richText":[raw]}));
    assert_eq!(
        project(None, &MediaRegistry::default(), &column, &record),
        ("Readable".into(), CellContent::Plain)
    );
}
