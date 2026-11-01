//! Supplied metadata disclosure only. No URL widget, state or action admission.
use super::*;

const MAX_DISPLAY_BYTES: usize = 4096;
fn supplied(field: &FormField, home: bool) -> Option<&str> {
    if !home
        || !matches!(&field.kind, InputKind::Unsupported { kind } if kind == "url_text_input")
        || field.raw["type"] != "url_text_input"
    {
        return None;
    }
    let value = field.raw.get("initial_value")?.as_str()?;
    if value.is_empty() {
        None
    } else if value.len() > MAX_DISPLAY_BYTES {
        Some("The initial URL is too long to display.")
    } else if value.contains('\0') {
        Some("The initial URL contains unsupported characters.")
    } else {
        Some(value)
    }
}

pub(super) fn view(field: &FormField, home: bool) -> Element {
    let Some(value) = supplied(field, home) else {
        return rsx! {};
    };
    // This is the raw supplied initial literal, never a saved/current value,
    // parsed destination, hyperlink or editable control. All form guards stay.
    rsx! {
        div {class:"app-modal-initial-url",
            span {class:"app-modal-hint","Initial URL (read only)"}
            p {class:"app-modal-initial-url-value",tabindex:"0",role:"region",
                "aria-label":"Initial URL (read only)","data-home-initial-url":"true",
                "{value}"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource_reads::render_test::text;
    use dioxus::core::{DynamicNode, TemplateNode};
    use serde_json::json;
    use super_platinum_core::block_actions::form::{FormError, home::HomeForm};

    #[test]
    fn readonly_initial_url_is_literal_text_without_widget_link_or_form_admission() {
        let literal = "https://EXAMPLE.test/Équipe?q=<b>literal</b>&x=1#review";
        let raw = json!({"type":"home","id":"V_INITIAL_URL","blocks":[
            {"type":"input","block_id":"link","label":{"type":"plain_text","text":"Project URL"},
             "element":{"type":"url_text_input","action_id":"value","initial_value":literal}}
        ]});
        let form = HomeForm::parse(&raw).unwrap();
        let key = InputKey {
            block_id: "link".into(),
            action_id: "value".into(),
        };
        let field = &form.fields[&key];
        assert!(matches!(&field.kind,InputKind::Unsupported { kind } if kind=="url_text_input"));
        assert!(matches!(form.state(), Err(FormError::Unsupported(_))));
        let node = view(field, true).unwrap();
        assert!(text(&node).contains(literal));
        assert!(
            node.dynamic_nodes
                .iter()
                .all(|n| matches!(n, DynamicNode::Text(_)))
        );
        fn plain(node: &TemplateNode) -> bool {
            match node {
                TemplateNode::Element { tag, children, .. } => {
                    matches!(*tag, "div" | "span" | "p") && children.iter().all(plain)
                }
                _ => true,
            }
        }
        assert!(
            node.template.roots.iter().all(plain),
            "no link, input or action element"
        );
        assert!(
            text(&view(field, false).unwrap()).is_empty(),
            "Modal admission stays separate"
        );
        assert_eq!(form.raw_view(), &raw);
        assert!(matches!(form.state(), Err(FormError::Unsupported(_))));
        for value in [json!(null), json!(17), json!("")] {
            let mut changed = field.clone();
            changed.raw["initial_value"] = value;
            assert!(text(&view(&changed, true).unwrap()).is_empty());
        }
        let mut changed = field.clone();
        changed.raw["initial_value"] = json!("x".repeat(MAX_DISPLAY_BYTES + 1));
        let rendered = text(&view(&changed, true).unwrap());
        assert!(rendered.contains("too long to display") && rendered.len() < 200);
        changed.raw["initial_value"] = json!("https://example.test/\0hidden");
        assert!(text(&view(&changed, true).unwrap()).contains("unsupported characters"));
        changed.raw["type"] = json!("file_input");
        assert!(text(&view(&changed, true).unwrap()).is_empty());
        changed = field.clone();
        changed.kind = InputKind::Email;
        assert!(text(&view(&changed, true).unwrap()).is_empty());
    }
}
