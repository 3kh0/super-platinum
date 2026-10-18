//! Typed presentation decisions from the retained md/mp components. This is
//! not an anchor observation: native href resolution, emoji interpolation and
//! navigation custody remain renderer work.
use super::{Error, MountedData};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Label {
    Text(String),
    DeletedFile,
    PrivateFile,
    DeletedCanvas,
    PrivateCanvas,
    Untitled,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    File,
    Canvas,
    Deleted,
    Private,
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Anchor {
    pub file_id: String,
    // Original getter string, not native a.href or a guessed resolved URL.
    pub href: String,
    pub label: Label,
    pub icon: Icon,
    pub dimmed: bool,
    pub clickable: bool,
    pub deleted_tooltip: bool,
    pub badge: bool,
    pub mime_type: Option<String>,
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Presentation {
    Placeholder,
    Anchor(Anchor),
    // A real cached rich name is not permission to use the plain name. Its
    // owner-bound member/channel/date/Salesforce context remains required.
    RichCanvasTitle { raw: String },
}

pub fn project(data: &MountedData, model_loading: bool) -> Presentation {
    match data {
        MountedData::File {
            file_id,
            name,
            mime_type,
            deleted,
            file_not_found,
            permalink,
        } => {
            let unavailable = *deleted || *file_not_found;
            let label = if *deleted {
                Some(Label::DeletedFile)
            } else if *file_not_found {
                Some(Label::PrivateFile)
            } else {
                name.as_deref()
                    .map(trim)
                    .filter(|v| !v.is_empty())
                    .map(|v| Label::Text(v.into()))
            };
            if file_id.is_empty() || label.is_none() && !unavailable {
                return Presentation::Placeholder;
            }
            // md does not check model.isLoading(): actual supplied model fields
            // and placeholder admission decide it, unlike the Canvas component.
            Presentation::Anchor(Anchor {
                file_id: file_id.clone(),
                href: permalink.clone().unwrap_or_default(),
                label: label.expect("unavailable labels are nonempty"),
                icon: if *deleted {
                    Icon::Deleted
                } else if *file_not_found {
                    Icon::Private
                } else {
                    Icon::File
                },
                dimmed: unavailable,
                clickable: !unavailable,
                deleted_tooltip: *deleted,
                badge: false,
                mime_type: mime_type.clone(),
            })
        }
        MountedData::Document {
            file_id,
            name,
            rich_text_name,
            should_show_document,
            is_deleted,
            show_badge,
            permalink,
        } => {
            if model_loading {
                return Presentation::Placeholder;
            }
            let private = !should_show_document;
            // The retained component computes its rich title even for an
            // unavailable object. Keep its explicit dependency rather than
            // silently substituting a plain title or fabricated resolved label.
            if let Some(raw) = rich_text_name {
                return Presentation::RichCanvasTitle { raw: raw.clone() };
            }
            if !private && !is_deleted && name.as_deref() == Some("") {
                return Presentation::Placeholder;
            }
            let label = if *is_deleted {
                Label::DeletedCanvas
            } else if private {
                Label::PrivateCanvas
            } else {
                name.as_deref()
                    .map(|v| Label::Text(trim(v).into()))
                    .unwrap_or(Label::Untitled)
            };
            Presentation::Anchor(Anchor {
                file_id: file_id.clone(),
                href: permalink.clone().unwrap_or_default(),
                label,
                icon: if *is_deleted {
                    Icon::Deleted
                } else if private {
                    Icon::Private
                } else {
                    Icon::Canvas
                },
                dimmed: private || *is_deleted,
                // mp disables deleted clicks, but does not disable private clicks.
                clickable: !is_deleted,
                deleted_tooltip: *is_deleted,
                badge: *show_badge && !private && !is_deleted,
                mime_type: None,
            })
        }
    }
}
fn trim(value: &str) -> &str {
    // ECMAScript String.trim, not Rust's broader Unicode White_Space property.
    value.trim_matches(|c| {
        matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
    })
}

/// Finish the rich branch after source-compatible chunk text resolution.
/// This is presentation only; no resolved text can authorize a read or write.
pub(super) fn finish_rich(
    data: &MountedData,
    text: &str,
    loading: bool,
) -> Result<Presentation, Error> {
    let MountedData::Document {
        rich_text_name: Some(_),
        should_show_document,
        is_deleted,
        ..
    } = data
    else {
        return Err(Error::Unsupported);
    };
    if loading || (*should_show_document && !is_deleted && text.is_empty()) {
        return Ok(Presentation::Placeholder);
    }
    let mut plain = data.clone();
    if let MountedData::Document {
        name,
        rich_text_name,
        ..
    } = &mut plain
    {
        *name = Some(text.into());
        *rich_text_name = None;
    }
    Ok(project(&plain, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(name: Option<&str>, deleted: bool, private: bool) -> MountedData {
        MountedData::File {
            file_id: "F1".into(),
            name: name.map(str::to_owned),
            mime_type: None,
            deleted,
            file_not_found: private,
            permalink: None,
        }
    }
    fn canvas(name: Option<&str>, rich: Option<&str>, show: bool, deleted: bool) -> MountedData {
        MountedData::Document {
            file_id: "F1".into(),
            name: name.map(str::to_owned),
            rich_text_name: rich.map(str::to_owned),
            should_show_document: show,
            is_deleted: deleted,
            show_badge: true,
            permalink: Some(String::new()),
        }
    }
    fn anchor(data: &MountedData) -> Anchor {
        let Presentation::Anchor(value) = project(data, false) else {
            panic!("expected actual anchor branch")
        };
        value
    }
    #[test]
    fn file_absent_empty_and_unavailable_labels_follow_actual_component_admission() {
        for name in [None, Some(""), Some(" \u{feff} ")] {
            assert_eq!(
                project(&file(name, false, false), false),
                Presentation::Placeholder
            );
            let deleted = anchor(&file(name, true, true));
            assert_eq!(deleted.label, Label::DeletedFile);
            assert!(!deleted.clickable);
            let private = anchor(&file(name, false, true));
            assert_eq!(private.label, Label::PrivateFile);
            assert!(!private.clickable);
        }
        assert_eq!(
            project(&file(Some("Name"), false, false), true),
            project(&file(Some("Name"), false, false), false)
        );
    }
    #[test]
    fn canvas_absent_empty_whitespace_and_loading_are_distinct() {
        assert_eq!(
            anchor(&canvas(None, None, true, false)).label,
            Label::Untitled
        );
        assert_eq!(
            project(&canvas(Some(""), None, true, false), false),
            Presentation::Placeholder
        );
        assert_eq!(
            anchor(&canvas(Some("   "), None, true, false)).label,
            Label::Text(String::new())
        );
        assert_eq!(
            project(&canvas(Some("Name"), None, true, false), true),
            Presentation::Placeholder
        );
        assert_eq!(anchor(&canvas(None, None, true, false)).href, "");
    }
    #[test]
    fn canvas_private_and_deleted_keep_distinct_click_and_badge_semantics() {
        let private = anchor(&canvas(Some(""), None, false, false));
        assert_eq!(private.label, Label::PrivateCanvas);
        assert!(private.clickable && private.dimmed && !private.badge);
        let deleted = anchor(&canvas(Some(""), None, false, true));
        assert_eq!(deleted.label, Label::DeletedCanvas);
        assert!(!deleted.clickable && deleted.deleted_tooltip);
        assert!(anchor(&canvas(Some("Name"), None, true, false)).badge);
    }
    #[test]
    fn rich_name_presence_never_uses_plain_title_as_resolved_fallback() {
        for rich in ["", "[]", "[{\"type\":\"rich_text\"}]"] {
            assert_eq!(
                project(&canvas(Some("fallback"), Some(rich), true, false), false),
                Presentation::RichCanvasTitle { raw: rich.into() }
            );
        }
    }
    #[test]
    fn rich_empty_private_deleted_and_whitespace_follow_final_source_branch() {
        assert_eq!(
            finish_rich(
                &canvas(Some("fallback"), Some("bad"), true, false),
                "",
                false
            )
            .unwrap(),
            Presentation::Placeholder
        );
        let Presentation::Anchor(private) =
            finish_rich(&canvas(None, Some(""), false, false), "", false).unwrap()
        else {
            panic!("private")
        };
        assert_eq!(private.label, Label::PrivateCanvas);
        assert!(private.clickable);
        let Presentation::Anchor(deleted) =
            finish_rich(&canvas(None, Some(""), false, true), "", false).unwrap()
        else {
            panic!("deleted")
        };
        assert_eq!(deleted.label, Label::DeletedCanvas);
        assert!(!deleted.clickable);
        let Presentation::Anchor(blank) =
            finish_rich(&canvas(None, Some(""), true, false), "  ", false).unwrap()
        else {
            panic!("whitespace")
        };
        assert_eq!(blank.label, Label::Text(String::new()));
        assert!(finish_rich(&canvas(None, None, true, false), "supplied", false).is_err());
    }
    #[test]
    fn javascript_trim_preserves_nel_and_zero_width_space_but_removes_bom() {
        assert_eq!(
            anchor(&file(Some("\u{feff}Name\u{feff}"), false, false)).label,
            Label::Text("Name".into())
        );
        for literal in ["\u{85}", "\u{200b}"] {
            assert_eq!(
                anchor(&file(Some(literal), false, false)).label,
                Label::Text(literal.into())
            );
        }
    }
}
