use super::*;
use serde_json::json;

fn scene() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    let w = s.core.workspaces.get_mut("T1").unwrap();
    for (name, value) in [
        ("wave", "https://fixture.invalid/standard-collision.png"),
        ("activity_ready", "https://fixture.invalid/ready.png"),
        ("activity_ready_alias", "alias:activity_ready"),
        ("activity_cold", "https://fixture.invalid/cold.png"),
    ] {
        w.custom_emoji.insert(
            name.into(),
            serde_json::from_value(json!({"name":name,"value":value})).unwrap(),
        );
    }
    s
}

fn ready(s: &ShellState) -> MediaAssetId {
    let Inline::Emoji {
        image: Some(id), ..
    } = single(s, "T1", ":activity_ready_alias:", true)
    else {
        panic!("alias must resolve");
    };
    s.media
        .insert(id.clone(), "image/svg+xml", b"<svg/>".to_vec());
    id
}

#[test]
fn semantic_title_and_ordered_painted_fragments_are_distinct() {
    let s = scene();
    let id = ready(&s);
    let label = "R\u{e9}vision & \u{4e2d} :wave: :activity_ready_alias: :unknown:";
    let painted = Painted::project(&s, "T1", label, s.media_epoch);
    assert_eq!(painted.label, label);
    assert_eq!(
        serde_json::to_value(&painted.fragments).unwrap(),
        json!([
            {"kind":"text","text":"R\u{e9}vision & \u{4e2d} "},
            {"kind":"emoji","text":"\u{1f44b}","title":":wave:"},
            {"kind":"text","text":" "},
            {"kind":"image","src":id.webview_uri_at(s.media_epoch),"alt":":activity_ready_alias:","title":":activity_ready_alias:"},
            {"kind":"text","text":" "},
            {"kind":"text","text":":unknown:"}
        ])
    );
    assert!(painted.current(&s, "T1"));
}

#[test]
fn cold_custom_has_readable_tooltip_and_does_not_borrow_another_team() {
    let s = scene();
    let painted = Painted::project(
        &s,
        "T1",
        "Canvas & caf\u{e9} :wave: :activity_cold: :unknown:",
        s.media_epoch,
    );
    assert_eq!(painted.fragments.len(), 6);
    assert_eq!(
        painted.fragments[3],
        Fragment::Emoji {
            text: ":activity_cold:".into(),
            title: ":activity_cold:".into()
        }
    );
    assert_eq!(
        Painted::project(&s, "TOTHER", ":activity_cold:", s.media_epoch).fragments,
        vec![Fragment::Text {
            text: ":activity_cold:".into()
        }]
    );
}

#[test]
fn painted_title_never_decodes_entities_or_parses_markup_or_unsupported_tones() {
    let s = scene();
    let label = "&amp;lt;<Review> _literal_ \u{4e2d} :wave::skin-tone-3:";
    let painted = Painted::project(&s, "T1", label, s.media_epoch);
    assert_eq!(painted.label, label);
    assert_eq!(
        painted.fragments,
        vec![
            Fragment::Text {
                text: "&amp;lt;<Review> _literal_ \u{4e2d} ".into()
            },
            Fragment::Text {
                text: ":wave::skin-tone-3:".into()
            }
        ]
    );
}

#[test]
fn readiness_epoch_and_workspace_emoji_changes_retire_painted_admission() {
    let mut s = scene();
    let cold = Painted::project(&s, "T1", ":activity_ready_alias:", s.media_epoch);
    let id = ready(&s);
    assert!(!cold.current(&s, "T1"));
    let loaded = Painted::project(&s, "T1", ":activity_ready_alias:", s.media_epoch);
    assert!(loaded.current(&s, "T1"));
    s.media_epoch += 1;
    assert!(!loaded.current(&s, "T1"));
    let loaded = Painted::project(&s, "T1", ":activity_ready_alias:", s.media_epoch);
    let revision = s.media.image_state(&id).0.unwrap();
    assert!(s.media.reject_image(&id, revision));
    assert!(!loaded.current(&s, "T1"));
    assert!(matches!(
        Painted::project(&s, "T1", ":activity_ready_alias:", s.media_epoch).fragments[0],
        Fragment::Emoji { .. }
    ));
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .custom_emoji
        .remove("activity_ready_alias");
    assert!(!cold.current(&s, "T1"));
}
