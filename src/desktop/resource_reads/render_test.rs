//! Actual component bodies/listeners, without native input or layout claims.
use crate::state::ShellState;
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode};
use dioxus::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use super_platinum_core::config::{Session, WorkspaceSession};
pub(crate) type Captured = Rc<RefCell<Option<VNode>>>;
fn shell(alias: &'static str) -> ShellState {
    let mut shell = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
    let team = shell.core.active_team.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    shell.core.active_account = Some("isolated-resource-read".into());
    shell.core.transport = Some(crate::bootstrap::test_support::transport(
        "synthetic-resource-read",
    ));
    shell.core.session = Some(Session {
        d_cookie: "synthetic-resource-read".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [(
            team.clone(),
            WorkspaceSession {
                team_id: team,
                user_id: user,
                name: "Fixture".into(),
                url: "https://fixture.invalid".into(),
                token: "synthetic-resource-read".into(),
                enterprise_id: None,
            },
        )]
        .into(),
    });
    shell
}
pub(crate) fn mount(
    alias: &'static str,
    render: fn(Signal<ShellState>) -> Element,
) -> (VirtualDom, Signal<ShellState>, Captured) {
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let captured = Captured::default();
    let mut dom = VirtualDom::new_with_props(
        |(alias, render, slot, captured): (
            &'static str,
            fn(Signal<ShellState>) -> Element,
            Slot,
            Captured,
        )| {
            let state = use_signal(|| shell(alias));
            slot.set(Some(state));
            let rendered = render(state);
            *captured.borrow_mut() = Some(rendered.as_ref().unwrap().clone());
            rendered
        },
        (alias, render, slot.clone(), captured.clone()),
    );
    dom.rebuild_to_vec();
    (dom, slot.get().unwrap(), captured)
}
pub(crate) struct Control {
    pub(crate) click: AttributeValue,
    pub(crate) disabled: bool,
}
pub(crate) fn button(v: &VNode, id: &str) -> Option<Control> {
    fn at(v: &VNode, node: &TemplateNode, id: &str) -> Option<Control> {
        match node {
            TemplateNode::Element {
                attrs, children, ..
            } => {
                let mut found = false;
                let mut click = None;
                let mut disabled = false;
                for attr in attrs.iter() {
                    match attr {
                        TemplateAttribute::Static { name, value, .. } => {
                            if *name == "id" && *value == id {
                                found = true;
                            }
                            if *name == "disabled" {
                                disabled = *value != "false";
                            }
                        }
                        TemplateAttribute::Dynamic { id: dynamic } => {
                            for attr in v.dynamic_attrs[*dynamic].iter() {
                                if attr.name == "id"
                                    && matches!(&attr.value,AttributeValue::Text(value) if value==id)
                                {
                                    found = true;
                                }
                                if attr.name == "onclick" {
                                    click = Some(attr.value.clone());
                                }
                                if attr.name == "disabled" {
                                    disabled = matches!(&attr.value, AttributeValue::Bool(true));
                                }
                            }
                        }
                    }
                }
                if found {
                    return click.map(|click| Control { click, disabled });
                }
                children.iter().find_map(|n| at(v, n, id))
            }
            TemplateNode::Dynamic { id: dynamic } => match &v.dynamic_nodes[*dynamic] {
                DynamicNode::Fragment(nodes) => nodes.iter().find_map(|v| button(v, id)),
                _ => None,
            },
            _ => None,
        }
    }
    v.template.roots.iter().find_map(|n| at(v, n, id))
}
pub(crate) fn text(v: &VNode) -> String {
    fn at(v: &VNode, node: &TemplateNode, out: &mut String) {
        match node {
            TemplateNode::Text { text } => out.push_str(text),
            TemplateNode::Element { children, .. } => {
                for n in children.iter() {
                    at(v, n, out)
                }
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Text(t) => out.push_str(&t.value),
                DynamicNode::Fragment(nodes) => {
                    for n in nodes.iter() {
                        out.push_str(&text(n))
                    }
                }
                _ => {}
            },
        }
    }
    let mut out = String::new();
    for node in v.template.roots.iter() {
        at(v, node, &mut out);
    }
    out
}
pub(crate) fn click(dom: &VirtualDom, control: Control) {
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    let AttributeValue::Listener(callback) = control.click else {
        panic!("expected actual click listener")
    };
    dom.in_scope(dioxus::core::ScopeId::ROOT, || {
        callback.call(dioxus::core::Event::new(
            dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default()).into_any(),
            true,
        ))
    });
}
pub(crate) async fn settle(dom: &mut VirtualDom, done: impl Fn() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            dom.render_immediate_to_vec();
            if done() {
                break;
            }
            dom.wait_for_work().await;
        }
    })
    .await
    .expect("actual resource callback settles through disconnected transport");
}
