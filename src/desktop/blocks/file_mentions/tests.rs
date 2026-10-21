use super::*;
use crate::blocks::block_nodes;
use crate::media::MediaRegistry;
use serde_json::json;

#[test]
fn whole_occurrences_preserve_source_team_raw_style_order_and_plain_text() {
    let core = crate::fixture::fixture_core();
    let w = core.workspaces.get("T1").unwrap();
    let media = MediaRegistry::default();
    let a = json!({"type":"canvas","file_id":"FILE1","text":"Canvas wire",
        "style":{"bold":true},"opaque":{"occurrence":1}});
    let b = json!({"type":"file","file_id":"FILE2","url":"https://example.test/list",
        "style":{"italic":true},"opaque":{"occurrence":2}});
    let raw = json!({"type":"rich_text","elements":[{"type":"rich_text_section",
        "elements":[a.clone(),{"type":"text","text":" / "},b.clone(),a.clone()]}]});
    let nodes = block_nodes(BlockCtx::new(w, &media), &raw);
    let [RichNode::Paragraph(children)] = nodes.as_slice() else {
        panic!("missing paragraph")
    };
    assert_eq!(children.len(), 4);
    for (i, original) in [(0, &a), (2, &b), (3, &a)] {
        let RichNode::FileMention { team, raw, .. } = &children[i] else {
            panic!("file token dropped")
        };
        assert_eq!(team, &w.team_id);
        assert_eq!(raw, original);
    }
    assert_eq!(
        nodes[0].plain_text(),
        "Canvas wire / https://example.test/listCanvas wire"
    );
}
#[test]
fn unsupported_canvas_variant_remains_visible_without_whole_file_authority() {
    let core = crate::fixture::fixture_core();
    let media = MediaRegistry::default();
    let ctx = BlockCtx::new(&core.workspaces["T1"], &media);
    for (key, value) in [
        ("section_id", json!("S1")),
        ("is_skill_invocation", json!(true)),
    ] {
        let mut raw = json!({"type":"canvas","file_id":"FILE1","text":"Original variant",
            "style":{"bold":true,"italic":true}});
        raw[key] = value;
        let result = node(ctx, &raw);
        assert!(matches!(
            result,
            RichNode::StyledText {
                bold: true,
                italic: true,
                ..
            }
        ));
        assert_eq!(result.plain_text(), "Original variant");
    }
}
#[test]
fn fallback_uses_only_original_text_url_or_identity_and_never_raw_title() {
    for (raw, want) in [
        (
            json!({"type":"file","text":"Wire","url":"https://example.test","file_id":"F1","title":"Wrong"}),
            "Wire",
        ),
        (
            json!({"type":"file","text":"","url":"https://example.test","file_id":"F1"}),
            "https://example.test",
        ),
        (json!({"type":"file","file_id":"F1","title":"Wrong"}), "F1"),
        (json!({"type":"canvas","title":"Wrong"}), "Canvas"),
    ] {
        assert_eq!(fallback(&raw), want);
    }
}
