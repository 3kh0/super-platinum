use super::*;
use serde_json::json;

fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    let w = s.core.workspaces.get_mut("T1").unwrap();
    for (name, value) in [
        ("wave", "https://fixture.invalid/collision.png"),
        ("shared_base", "https://fixture.invalid/emoji.png"),
        ("shared_alias", "alias:shared_base"),
    ] {
        w.custom_emoji.insert(
            name.into(),
            serde_json::from_value(json!({"name":name,"value":value})).unwrap(),
        );
    }
    s
}

#[test]
fn shared_projection_keeps_exact_six_parts_and_standard_precedence() {
    let s = shell();
    let parts = project(
        &s,
        "T1",
        "Title & \u{4e2d} :wave: :shared_alias: :unknown:",
        true,
    );
    assert_eq!(parts.len(), 6);
    assert_eq!(parts[0], Part::Text("Title & \u{4e2d} ".into()));
    assert_eq!(
        parts[1],
        Part::Emoji {
            text: "\u{1f44b}".into(),
            title: Some(":wave:".into()),
            image: None
        }
    );
    assert_eq!(parts[2], Part::Text(" ".into()));
    let Part::Emoji {
        text,
        title,
        image: Some(id),
    } = &parts[3]
    else {
        panic!("alias image missing")
    };
    assert_eq!(text, ":shared_alias:");
    assert_eq!(title.as_deref(), Some(":shared_alias:"));
    assert!(
        !s.media.is_ready(id),
        "lookup alone does not invent ready bytes"
    );
    let Part::Emoji {
        image: Some(base), ..
    } = single(&s, "T1", ":shared_base:", true)
    else {
        panic!("base missing")
    };
    // The media registry deduplicates the resolved URL without erasing the
    // original occurrence's display alias or tooltip.
    assert_eq!(id, &base);
    assert_eq!(parts[4], Part::Text(" ".into()));
    assert_eq!(parts[5], Part::Text(":unknown:".into()));
}

#[test]
fn project_does_not_decode_parse_markup_trim_or_borrow_another_workspace() {
    let s = shell();
    let text = "  &amp;lt; <b> _literal_ \u{4e2d} :wave::skin-tone-3: ";
    let parts = project(&s, "T1", text, true);
    assert_eq!(
        parts,
        vec![
            Part::Text("  &amp;lt; <b> _literal_ \u{4e2d} ".into()),
            Part::Text(":wave::skin-tone-3:".into()),
            Part::Text(" ".into())
        ]
    );
    assert_eq!(
        single(&s, "TOTHER", ":shared_alias:", true),
        Part::Text(":shared_alias:".into())
    );
    assert_eq!(project(&s, "T1", "", true), vec![Part::Text("".into())]);
}

#[test]
fn tooltip_policy_is_explicit_without_changing_lookup() {
    let s = shell();
    assert_eq!(
        single(&s, "T1", ":WAVE:", false),
        Part::Emoji {
            text: "\u{1f44b}".into(),
            title: None,
            image: None
        }
    );
    assert_eq!(
        single(&s, "T1", ":WAVE:", true),
        Part::Emoji {
            text: "\u{1f44b}".into(),
            title: Some(":WAVE:".into()),
            image: None
        }
    );
}
