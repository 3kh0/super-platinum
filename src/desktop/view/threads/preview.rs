use super::*;
use crate::resource_reads::render_test::{mount, text};
use dioxus::core::{AttributeValue, DynamicNode, TemplateAttribute, TemplateNode};

fn render(state: Signal<ShellState>) -> Element {
    thread_list(state, &state.read())
}

// Inspect the actual generated tree, including nested fragment attributes.
#[derive(Default)]
struct Shape {
    tags: Vec<String>,
    actions: usize,
    destinations: Vec<String>,
    bodies: usize,
}
fn shape(v: &VNode, only_body: bool) -> Shape {
    fn at(v: &VNode, node: &TemplateNode, result: &mut Shape, only_body: bool, inside: bool) {
        match node {
            TemplateNode::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let is_body = attrs.iter().any(|attr| match attr {
                    TemplateAttribute::Static {name:"class",value,..} => value.split_whitespace().any(|c|c=="thread-feed-root-body"),
                    TemplateAttribute::Dynamic {id} => v.dynamic_attrs[*id].iter().any(|attr|attr.name=="class" && matches!(&attr.value,AttributeValue::Text(value) if value.split_whitespace().any(|c|c=="thread-feed-root-body"))),
                    _ => false,
                });
                if is_body {
                    result.bodies += 1;
                }
                let inside = inside || is_body;
                let collect = !only_body || inside;
                if collect {
                    result.tags.push((*tag).into());
                    for attr in attrs.iter() {
                        match attr {
                            TemplateAttribute::Static { name, value, .. } => {
                                if matches!(*name, "href" | "src") {
                                    result.destinations.push((*value).into());
                                }
                            }
                            TemplateAttribute::Dynamic { id } => {
                                for attr in v.dynamic_attrs[*id].iter() {
                                    if matches!(attr.value, AttributeValue::Listener(_)) {
                                        result.actions += 1;
                                    }
                                    if matches!(attr.name, "href" | "src")
                                        && let AttributeValue::Text(value) = &attr.value
                                    {
                                        result.destinations.push(value.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                for child in children.iter() {
                    at(v, child, result, only_body, inside);
                }
            }
            TemplateNode::Dynamic { id } => {
                if let DynamicNode::Fragment(nodes) = &v.dynamic_nodes[*id] {
                    for child in nodes.iter() {
                        for root in child.template.roots.iter() {
                            at(child, root, result, only_body, inside);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut result = Shape::default();
    for root in v.template.roots.iter() {
        at(v, root, &mut result, only_body, false);
    }
    result
}

#[test]
fn generated_global_root_is_readable_and_inert_without_seeding_history_or_media_and_tracks_edits_and_deletion()
 {
    for alias in ["thread-feed-rich-root", "thread-feed-rich-root-narrow"] {
        let (mut dom, mut state, captured) = mount(alias, render);
        let before = {
            let s = state.peek();
            (
                serde_json::to_value(&s.core.threads_view.items).unwrap(),
                format!("{:?}", s.core.threads),
                s.core.composer.text.clone(),
                s.core.thread_composer.text.clone(),
                s.core.threads_view.load_seq,
            )
        };
        let root = state.peek().core.threads_view.items[0].root_msg.clone();
        let rendered = text(captured.borrow().as_ref().unwrap());
        for expected in [
            "Release notes",
            "multiple paragraphs",
            "Review the changes",
            "Read the notes",
            "cargo test --locked",
            "Keep the release discussion",
            "Release diagram",
            "Review action",
            "147 replies",
            "2 unread",
            "Open thread",
        ] {
            assert!(rendered.contains(expected), "{expected}");
        }
        assert!(!rendered.contains("Fallback must not duplicate"));
        let page = shape(captured.borrow().as_ref().unwrap(), false);
        let actual = shape(captured.borrow().as_ref().unwrap(), true);
        assert_eq!(
            actual.bodies, 1,
            "inspect the actual root body, not its author avatar"
        );
        assert!(page.tags.iter().any(|t| t == "article"));
        for tag in ["p", "ol", "li", "code", "blockquote"] {
            assert!(actual.tags.iter().any(|t| t == tag), "missing {tag}");
        }
        assert!(
            !actual
                .tags
                .iter()
                .any(|t| matches!(t.as_str(), "a" | "img" | "input" | "form" | "button"))
        );
        assert!(actual.destinations.is_empty());
        assert_eq!(actual.actions, 0, "body has no handlers of any kind");
        assert_eq!(
            page.tags.iter().filter(|t| t.as_str() == "img").count(),
            1,
            "the existing ready author avatar is outside the body"
        );
        assert_eq!(
            page.tags.iter().filter(|t| t.as_str() == "button").count(),
            3
        );
        assert_eq!(
            page.actions, 3,
            "only existing refresh, explicit open and older controls"
        );
        {
            let s = state.peek();
            assert_eq!(
                s.media
                    .registered_source_count("https://fixture.invalid/root.png"),
                0
            );
            assert_eq!(
                before,
                (
                    serde_json::to_value(&s.core.threads_view.items).unwrap(),
                    format!("{:?}", s.core.threads),
                    s.core.composer.text.clone(),
                    s.core.thread_composer.text.clone(),
                    s.core.threads_view.load_seq,
                )
            );
        }
        let channel = root.channel.clone().unwrap();
        let ts = root.ts.clone().unwrap();
        let mut edit = root;
        edit.text = Some("Edited first line\nEdited second line".into());
        edit.blocks = vec![
            serde_json::json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Edited first line\nEdited second line"}]}]}),
        ];
        state.write().core.threads_view.observe(
            &super_platinum_core::slack::events::RtEvent::MessageChanged {
                channel: channel.clone(),
                message: edit,
                original_update: None,
            },
        );
        dom.render_immediate_to_vec();
        let rendered = text(captured.borrow().as_ref().unwrap());
        assert!(rendered.contains("Edited first line") && rendered.contains("Edited second line"));
        assert!(!rendered.contains("Release notes"));
        state.write().core.threads_view.observe(
            &super_platinum_core::slack::events::RtEvent::MessageDeleted {
                channel,
                deleted_ts: ts,
            },
        );
        dom.render_immediate_to_vec();
        let rendered = text(captured.borrow().as_ref().unwrap());
        assert!(rendered.contains("No followed threads yet."));
        assert!(!rendered.contains("Edited first line"));
    }
}

mod reactions;
