//! Board-only glyph/title projection; canonical group labels and membership stay unchanged.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckboxHeading {
    pub checked: bool,
    pub accessible_label: String,
}

/// Called only after successful saved-view projection. Never infer kind from a label.
pub(super) fn for_lane(
    document: &Document,
    view_id: Option<&str>,
    lane_key: Option<&str>,
) -> Option<CheckboxHeading> {
    let view_id = view_id?;
    let raw = document
        .views
        .iter()
        .find(|view| view.get("id").and_then(Value::as_str) == Some(view_id))?;
    if raw.get("type").and_then(Value::as_str) != Some("board") {
        return None;
    }
    let key = raw.pointer("/grouping/group_by")?.as_str()?;
    let column = document
        .columns
        .iter()
        .find(|column| column.key == key && column.kind == "checkbox")?;
    // B56 BoardLaneHeader uses an unchecked glyph for empty; its separate
    // title helper returns Ungrouped. True uses the typed field renderer.
    match lane_key? {
        "" => Some(CheckboxHeading {
            checked: false,
            accessible_label: "Ungrouped".into(),
        }),
        "true" => Some(CheckboxHeading {
            checked: true,
            accessible_label: format!("{}: true", column.name),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn document(kind: &str, layout: &str) -> Document {
        Document::parse(json!({"id":"F_HEADINGS","mode":"list","title":"Headings",
            "list_metadata":{"schema":[{"id":"C_CHECK","key":"done","name":"Delivery verified","type":kind}],
                "views":[{"id":"V_BOARD","name":"Completion","type":layout,"grouping":{"group_by":"done"}}]}}))
            .unwrap()
    }

    #[test]
    fn ordinary_checkbox_heading_separates_visual_state_from_accessible_title() {
        let doc = document("checkbox", "board");
        assert_eq!(
            for_lane(&doc, Some("V_BOARD"), Some("")),
            Some(CheckboxHeading {
                checked: false,
                accessible_label: "Ungrouped".into(),
            })
        );
        assert_eq!(
            for_lane(&doc, Some("V_BOARD"), Some("true")),
            Some(CheckboxHeading {
                checked: true,
                accessible_label: "Delivery verified: true".into(),
            })
        );
        assert!(for_lane(&doc, Some("V_BOARD"), Some("false")).is_none());
        assert!(for_lane(&doc, Some("V_BOARD"), None).is_none());
        assert!(for_lane(&doc, Some("removed"), Some("")).is_none());
    }

    #[test]
    fn table_select_and_todo_titles_keep_their_existing_presentation() {
        for (kind, layout) in [
            ("checkbox", "table"),
            ("select", "board"),
            ("todo_completed", "board"),
        ] {
            let doc = document(kind, layout);
            for key in ["", "true"] {
                assert!(for_lane(&doc, Some("V_BOARD"), Some(key)).is_none());
            }
        }
    }
}
