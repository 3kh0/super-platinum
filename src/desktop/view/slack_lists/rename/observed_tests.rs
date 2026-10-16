//! Actual panel RSX placement from representative renderer presentations.
//! This tests structure, not canonical readback, WebView layout or focus.
use super::*;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use super_platinum_core::slack_lists::ColumnCreateKind;

#[derive(Default, Debug)]
struct Node {
    attrs: BTreeMap<String, String>,
    children: Vec<Node>,
    text: String,
}
impl Node {
    fn attr(&self, name: &str) -> &str {
        self.attrs.get(name).map(String::as_str).unwrap_or("")
    }
    fn text(&self) -> String {
        self.text.clone() + &self.children.iter().map(Node::text).collect::<String>()
    }
    fn find<'a>(&'a self, name: &str, value: &str, found: &mut Vec<&'a Node>) {
        if self.attr(name) == value {
            found.push(self);
        }
        for child in &self.children {
            child.find(name, value, found);
        }
    }
    fn all(&self, name: &str, value: &str) -> Vec<&Node> {
        let mut found = Vec::new();
        self.find(name, value, &mut found);
        found
    }
}
fn expand(v: &VNode) -> Vec<Node> {
    v.template
        .roots
        .iter()
        .flat_map(|n| expand_node(v, n))
        .collect()
}
fn expand_node(v: &VNode, node: &TemplateNode) -> Vec<Node> {
    match node {
        TemplateNode::Element {
            attrs, children, ..
        } => {
            let mut out = Node::default();
            for attr in attrs.iter() {
                match attr {
                    TemplateAttribute::Static { name, value, .. } => {
                        out.attrs.insert(name.to_string(), value.to_string());
                    }
                    TemplateAttribute::Dynamic { id } => {
                        for a in v.dynamic_attrs[*id].iter() {
                            let value = match &a.value {
                                AttributeValue::Text(t) => Some(t.clone()),
                                AttributeValue::Bool(b) => Some(b.to_string()),
                                _ => None,
                            };
                            if let Some(value) = value {
                                out.attrs.insert(a.name.into(), value);
                            }
                        }
                    }
                }
            }
            out.children = children.iter().flat_map(|n| expand_node(v, n)).collect();
            vec![out]
        }
        TemplateNode::Text { text } => vec![Node {
            text: text.to_string(),
            ..Default::default()
        }],
        TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
            DynamicNode::Fragment(nodes) => nodes.iter().flat_map(expand).collect(),
            DynamicNode::Text(text) => vec![Node {
                text: text.value.clone(),
                ..Default::default()
            }],
            DynamicNode::Placeholder(_) => vec![],
            DynamicNode::Component(_) => {
                panic!("Panel unexpectedly introduced a component boundary")
            }
        },
    }
}

#[test]
fn exact_observed_summary_is_outside_body_only_for_select_and_options_remain_locked() {
    for kind in [
        ColumnCreateKind::Select,
        ColumnCreateKind::Text,
        ColumnCreateKind::NumberDefault0,
        ColumnCreateKind::Checkbox,
        ColumnCreateKind::DateSingleDefault,
    ] {
        for observed in [false, true] {
            let result = Rc::new(RefCell::new(None));
            let mut dom = VirtualDom::new_with_props(
                |(result, kind, observed): (Rc<RefCell<Option<Node>>>, ColumnCreateKind, bool)| {
                    let state = use_signal(|| {
                        ShellState::fixture_variant(
                            crate::media::MediaRegistry::default(),
                            "slack-list-select-field-create",
                        )
                    });
                    let mut view = rename::presentation(&state.peek()).unwrap();
                    // Renderer setup only: production panel() still obtains the actual presentation.
                    view.field_choice.as_mut().unwrap().kind = kind;
                    if kind != ColumnCreateKind::Select {
                        view.select_draft = None;
                    }
                    view.stage = if observed {
                        Stage::Observed
                    } else {
                        Stage::Uncertain
                    };
                    view.can_confirm = false;
                    view.current_review = observed;
                    view.observed_field = observed.then(|| rename::text_create::Observed {
                        id: "ACTUAL_COLUMN".into(),
                        key: "actual_key".into(),
                        name: "Release phase".into(),
                    });
                    let rendered = render_panel(state, view);
                    let nodes = expand(rendered.as_ref().unwrap());
                    *result.borrow_mut() = Some(Node {
                        children: nodes,
                        ..Default::default()
                    });
                    assert!(rename::text_create::select::close_fixture(&state.peek()));
                    rendered
                },
                (result.clone(), kind, observed),
            );
            dom.rebuild_to_vec();
            let tree = result.borrow_mut().take().unwrap();
            let panel = tree.all("id", "slack-list-rename")[0];
            let body = panel.all("class", "slack-list-editor-body")[0];
            let summaries = panel.all("id", "slack-list-text-field-observed");
            assert_eq!(summaries.len(), usize::from(observed));
            assert_eq!(
                body.all("id", "slack-list-text-field-observed").len(),
                usize::from(observed && kind != ColumnCreateKind::Select)
            );
            if observed {
                let summary = summaries[0];
                assert_eq!(summary.attr("data-column-id"), "ACTUAL_COLUMN");
                assert_eq!(summary.attr("data-column-key"), "actual_key");
                assert_eq!(
                    summary.text(),
                    format!(
                        "Observed {} field: Release phase",
                        rename::text_create::label(kind)
                    )
                );
                if kind == ColumnCreateKind::Select {
                    let index = panel
                        .children
                        .iter()
                        .position(|n| n.attr("id") == "slack-list-text-field-observed")
                        .unwrap();
                    assert_eq!(
                        panel.children[index - 1].attr("class"),
                        "slack-list-editor-body"
                    );
                    assert_eq!(
                        panel.children[index + 1].attr("class"),
                        "slack-list-editor-actions"
                    );
                }
            }
            if kind == ColumnCreateKind::Select {
                let options = body.all("id", "slack-list-select-options")[0];
                assert_eq!(options.attr("disabled"), "true");
                let rows = options.all("class", "slack-list-select-option");
                assert_eq!(rows.len(), 8);
                for row in rows {
                    assert!(!row.attr("data-option-id").is_empty());
                    assert!(!row.attr("data-option-generation").is_empty());
                    assert_eq!(row.all("disabled", "true").len(), 1);
                }
            }
        }
    }
}

#[test]
fn multi_cardinality_remains_checked_and_locked_in_observed_panel_with_same_summary_placement() {
    let result = Rc::new(RefCell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |result: Rc<RefCell<Option<Node>>>| {
            let state = use_signal(|| {
                ShellState::fixture_variant(
                    crate::media::MediaRegistry::default(),
                    "slack-list-multi-select-field-create",
                )
            });
            let mut view = rename::presentation(&state.peek()).unwrap();
            assert_eq!(
                view.select_draft.as_ref().unwrap().format,
                super_platinum_core::slack_lists::SelectCreateFormat::Multiple
            );
            view.stage = Stage::Observed;
            view.can_confirm = false;
            view.current_review = true;
            view.observed_field = Some(rename::text_create::Observed {
                id: "OBSERVED".into(),
                key: "actual_key".into(),
                name: "Release phase".into(),
            });
            let rendered = render_panel(state, view);
            *result.borrow_mut() = Some(Node {
                children: expand(rendered.as_ref().unwrap()),
                ..Default::default()
            });
            assert!(rename::text_create::select::close_fixture(&state.peek()));
            rendered
        },
        result.clone(),
    );
    dom.rebuild_to_vec();
    let tree = result.borrow_mut().take().unwrap();
    let panel = tree.all("id", "slack-list-rename")[0];
    let checkbox = panel.all("id", "slack-list-select-multiple")[0];
    assert_eq!(checkbox.attr("type"), "checkbox");
    assert_eq!(checkbox.attr("checked"), "true");
    assert_eq!(checkbox.attr("disabled"), "true");
    assert!(panel.text().contains("Allow multiple selections"));
    let body = panel.all("class", "slack-list-editor-body")[0];
    assert!(body.all("id", "slack-list-text-field-observed").is_empty());
    assert_eq!(
        panel.all("id", "slack-list-text-field-observed")[0].text(),
        "Observed Select field: Release phase"
    );
}
