//! Actual typed rows/listeners; no native paint, input, network or layout claim.
use super::*;
use crate::{blocks::table::fixture, resource_reads::render_test as rt};
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode};

#[derive(Default)]
struct Facts {
    tags: Vec<String>,
    table_attrs: Vec<(String, String)>,
    table_listeners: usize,
    buttons: Vec<(String, Option<AttributeValue>)>,
}
fn node_text(v: &VNode, n: &TemplateNode) -> String {
    match n {
        TemplateNode::Text { text } => (*text).into(),
        TemplateNode::Element { children, .. } => {
            children.iter().map(|n| node_text(v, n)).collect()
        }
        TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
            DynamicNode::Text(t) => t.value.clone(),
            DynamicNode::Fragment(nodes) => nodes.iter().map(rt::text).collect(),
            _ => String::new(),
        },
    }
}
fn facts(v: &VNode) -> Facts {
    fn at(v: &VNode, n: &TemplateNode, inside: bool, out: &mut Facts) {
        match n {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let mut pairs = Vec::new();
                let mut click = None;
                let mut listeners = 0;
                for a in attrs.iter() {
                    match a {
                        TemplateAttribute::Static { name, value, .. } => {
                            pairs.push(((*name).into(), (*value).into()))
                        }
                        TemplateAttribute::Dynamic { id } => {
                            for a in v.dynamic_attrs[*id].iter() {
                                if matches!(&a.value, AttributeValue::Listener(_)) {
                                    listeners += 1;
                                    if a.name == "onclick" {
                                        click = Some(a.value.clone());
                                    }
                                }
                                if let AttributeValue::Text(s) = &a.value {
                                    pairs.push((a.name.into(), s.clone()));
                                }
                            }
                        }
                    }
                }
                let inside = inside
                    || *tag == "table"
                    || pairs
                        .iter()
                        .any(|(k, v)| k == "class" && v == "block-table-scroll");
                if inside {
                    out.tags.push((*tag).into());
                    out.table_attrs.extend(pairs);
                    out.table_listeners += listeners;
                }
                if *tag == "button" {
                    out.buttons.push((node_text(v, n), click));
                }
                for n in children.iter() {
                    at(v, n, inside, out);
                }
            }
            TemplateNode::Dynamic { id } => {
                if let DynamicNode::Fragment(nodes) = &v.dynamic_nodes[*id] {
                    for child in nodes.iter() {
                        for n in child.template.roots.iter() {
                            at(child, n, inside, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Facts::default();
    for n in v.template.roots.iter() {
        at(v, n, false, &mut out);
    }
    out
}
fn rows(state: Signal<ShellState>) -> Element {
    let shell = state.read();
    let channel = shell.core.active_channel.as_deref().unwrap();
    rsx! { for message in &shell.messages {
        {message_row(state,&shell,message,channel,None,RowSurface::Timeline,None)}
    }}
}
fn discussion_table(state: Signal<ShellState>) -> Element {
    let table = crate::blocks::table::parse(&fixture::block()).unwrap();
    super::discussion::body(state, 0, &RichNode::Table(table))
}
fn plain_tools(state: Signal<ShellState>) -> Element {
    let shell = state.read();
    let message = shell
        .messages
        .iter()
        .find(|m| m.ts == fixture::PLAIN_TS)
        .unwrap();
    message_tools(
        state,
        message,
        shell.core.active_channel.as_deref().unwrap(),
        RowSurface::Timeline,
    )
}

#[tokio::test]
async fn table_rows_are_reachable_inert_and_table_arrival_refuses_retained_edit_callback() {
    for alias in ["block-kit-static-table", "block-kit-static-table-narrow"] {
        let (_dom, state, captured) = rt::mount(alias, rows);
        let v = captured.borrow();
        let v = v.as_ref().unwrap();
        let f = facts(v);
        let text = rt::text(v);
        assert_eq!(f.tags.iter().filter(|t| *t == "table").count(), 1);
        for (tag, count) in [
            ("thead", 1),
            ("tbody", 1),
            ("tr", 13),
            ("th", 3),
            ("td", 36),
        ] {
            assert_eq!(
                f.tags.iter().filter(|t| t.as_str() == tag).count(),
                count,
                "{alias} {tag}"
            );
        }
        assert!(text.contains("Project 12"));
        assert!(text.contains("$12,000.00"));
        assert!(text.contains("Ready <literal>Release notes👍"));
        assert!(!text.contains("Table fallback must not duplicate"));
        assert!(text.contains("Unsupported table fallback"));
        assert_eq!(f.table_listeners, 0);
        assert!(
            !f.table_attrs
                .iter()
                .any(|(k, _)| matches!(k.as_str(), "href" | "src" | "dangerous_inner_html"))
        );
        assert!(
            f.table_attrs
                .iter()
                .any(|(k, v)| k == "tabindex" && v == "0")
        );
        assert_eq!(
            f.table_attrs
                .iter()
                .filter(|(k, v)| k == "scope" && v == "col")
                .count(),
            3
        );
        assert!(
            f.table_attrs
                .iter()
                .any(|(k, v)| k == "class" && v == "block-table-cell right nowrap")
        );
        assert!(
            f.table_attrs
                .iter()
                .any(|(k, v)| k == "class" && v == "block-table-cell left wrapped")
        );
        for tag in ["b", "i", "u", "s", "code", "ol", "blockquote", "pre"] {
            assert!(f.tags.iter().any(|t| t == tag));
        }
        assert_eq!(
            f.buttons.iter().filter(|(s, _)| s == "Edit").count(),
            1,
            "only ordinary message keeps Edit"
        );
        assert_eq!(
            f.buttons.iter().filter(|(s, _)| s == "Delete").count(),
            3,
            "other controls unchanged"
        );
        let shell = state.peek();
        assert!(
            shell
                .messages
                .iter()
                .find(|m| m.ts == fixture::TABLE_TS)
                .unwrap()
                .has_table_block
        );
        assert!(
            shell
                .messages
                .iter()
                .find(|m| m.ts == fixture::UNSUPPORTED_TS)
                .unwrap()
                .has_table_block
        );
    }
    let (_discussion, _, captured) = rt::mount("block-kit-static-table", discussion_table);
    assert_eq!(
        rt::text(captured.borrow().as_ref().unwrap()),
        crate::blocks::table::parse(&fixture::block())
            .unwrap()
            .plain_text()
    );
    let (dom, mut state, captured) = rt::mount("block-kit-static-table", plain_tools);
    let old = facts(captured.borrow().as_ref().unwrap())
        .buttons
        .into_iter()
        .find(|(s, _)| s == "Edit")
        .unwrap()
        .1
        .unwrap();
    let (team, channel) = {
        let s = state.peek();
        (
            s.core.active_team.clone().unwrap(),
            s.core.active_channel.clone().unwrap(),
        )
    };
    // Positive counterfactual: the original callback still opens an ordinary edit.
    rt::click(
        &dom,
        rt::Control {
            click: old.clone(),
            disabled: false,
        },
    );
    assert_eq!(
        state.peek().core.editing.as_ref(),
        Some(&(channel.clone(), fixture::PLAIN_TS.into()))
    );
    state.write().core.edit_composer.text = "Keep my newer draft".into();
    let generation = state.peek().edit_generation;
    let draft = state.peek().core.edit_composer.text.clone();
    let editing = state.peek().core.editing.clone();
    {
        let mut s = state.write();
        let message = s
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .messages
            .get_mut(&channel)
            .unwrap()
            .messages
            .iter_mut()
            .find(|m| m.ts.as_deref() == Some(fixture::PLAIN_TS))
            .unwrap();
        message.blocks = vec![fixture::block()];
        message.team = Some("contradictory-embedded-team".into());
        message.channel = Some("contradictory-embedded-channel".into());
    }
    rt::click(
        &dom,
        rt::Control {
            click: old,
            disabled: false,
        },
    );
    assert_eq!(state.peek().edit_generation, generation);
    assert_eq!(state.peek().core.edit_composer.text, draft);
    assert_eq!(state.peek().core.editing, editing);
}
