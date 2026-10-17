use super::*;
use serde_json::json;
use super_platinum_core::slack_lists::{Document, RecordsPage};
fn parts(value: Option<Value>) -> (Column, Record) {
    let doc=Document::parse(json!({"id":"F1","mode":"list","list_metadata":{"schema":[{"id":"C1","key":"rating","name":"Quality","type":"rating","options":{"emoji":":heart:","max":5}}]}})).unwrap();
    let fields = value.map_or(json!([]), |v| json!([{"key":"rating","rating":v}]));
    let page =
        RecordsPage::parse(json!({"ok":true,"records":[{"id":"R1","fields":fields}]})).unwrap();
    (doc.column("C1").unwrap().clone(), page.records[0].clone())
}
#[test]
fn missing_empty_and_legacy_zero_display_unrated_without_backfill_or_edit_authority() {
    let media = MediaRegistry::default();
    for value in [None, Some(json!([])), Some(json!([0]))] {
        let (column, record) = parts(value);
        let raw = record.raw.clone();
        let (text, content) = project(None, &media, &column, &record).unwrap();
        assert_eq!(text, "");
        assert!(matches!(
            content,
            CellContent::Rating {
                value: 0,
                max: 5,
                ..
            }
        ));
        assert_eq!(record.raw, raw);
    }
    let (column, record) = parts(Some(json!([3])));
    assert!(
        matches!(project(None,&media,&column,&record),Some((text,CellContent::Rating{value:3,max:5,..})) if text=="3")
    );
    let (column, record) = parts(Some(json!([6])));
    assert!(project(None, &media, &column, &record).is_none());
}
#[test]
fn custom_emoji_missing_from_current_workspace_has_readable_content_fallback() {
    assert!(
        matches!(glyph(None,&MediaRegistry::default(),":missing_custom:",Some("OTHER")),RichNode::Emoji{name,glyph} if name=="missing_custom" && glyph==":missing_custom:")
    );
    assert!(
        matches!(glyph(None,&MediaRegistry::default(),":star:",None),RichNode::Emoji{glyph,..} if glyph!=":star:")
    );
}
#[test]
fn cached_custom_emoji_uses_opaque_media_only_for_its_current_team() {
    let mut s = ShellState::fixture_variant(MediaRegistry::default(), "slack-list");
    let team = s.core.active_team.clone().unwrap();
    let workspace = s.core.workspaces.get_mut(&team).unwrap();
    workspace.custom_emoji.insert(
        "quality".into(),
        super_platinum_core::slack::models::Emoji {
            name: "quality".into(),
            value: "https://fixture.invalid/emoji-quality.png".into(),
            ..Default::default()
        },
    );
    assert!(
        matches!(glyph(Some(workspace),&s.media,":quality:",Some(&team)),RichNode::EmojiImage{name,..} if name=="quality")
    );
    assert!(
        matches!(glyph(Some(workspace),&s.media,":quality:",Some("OTHER")),RichNode::Emoji{glyph,..} if glyph==":quality:")
    );
}
