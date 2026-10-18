//! Actual Calendar renderer structure; no WebView geometry/focus claim.
//! This tests structure, not canonical readback, WebView layout or focus.
use super::*;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode, VNode};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

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
fn calendar_form_preserves_scope_controls_and_locked_unknown_draft() {
    for (modern, stage) in [
        (false, Stage::Editing),
        (true, Stage::Editing),
        (false, Stage::Creating),
        (false, Stage::Uncertain),
    ] {
        let output = Rc::new(RefCell::new(None));
        let mut dom = VirtualDom::new_with_props(
            |(output, modern, stage): (Rc<RefCell<Option<Node>>>, bool, Stage)| {
                let state = use_signal(move || {
                    let (mut s, id) = super::super::tests::setup();
                    let c = active_mut(&mut s, id).unwrap();
                    c.account = Some(if modern {
                        cal::Account::Modern
                    } else {
                        cal::Account::Legacy {
                            user: "cal-1".into(),
                        }
                    });
                    c.custom = false;
                    c.stage = stage;
                    s
                });
                let id = state
                    .read()
                    .message_buttons
                    .shortcuts
                    .dialog
                    .as_ref()
                    .unwrap()
                    .id;
                let rendered = render(state, id);
                *output.borrow_mut() = Some(Node {
                    children: expand(rendered.as_ref().unwrap()),
                    ..Default::default()
                });
                rendered
            },
            (output.clone(), modern, stage),
        );
        dom.rebuild_to_vec();
        let tree = output.borrow_mut().take().unwrap();
        assert_eq!(tree.all("id", "app-shortcut-dialog").len(), 1);
        assert_eq!(tree.all("id", "calendar-title").len(), 1);
        assert_eq!(tree.all("id", "calendar-guest-query").len(), 1);
        assert_eq!(tree.all("id", "calendar-calendar").len(), 1);
        assert_eq!(
            tree.all("id", "calendar-share-channel").len(),
            usize::from(!modern)
        );
        let duration = tree.all("id", "calendar-duration")[0];
        assert_eq!(
            duration
                .children
                .iter()
                .map(|n| n.attr("value"))
                .collect::<Vec<_>>(),
            vec!["15", "30", "45", "60", "90", "120"]
        );
        let field = tree.all("id", "calendar-title")[0];
        assert_eq!(
            field.attr("disabled"),
            (stage != Stage::Editing).to_string()
        );
        if stage == Stage::Uncertain {
            assert!(tree.all("id", "calendar-create").is_empty());
            assert!(tree.text().contains("Start a different event"));
        } else {
            let create = tree.all("id", "calendar-create")[0];
            assert_eq!(
                create.text(),
                if stage == Stage::Creating {
                    "Creating\u{2026}"
                } else {
                    "Create event"
                }
            );
        }
    }
}
