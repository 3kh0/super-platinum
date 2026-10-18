//! Retained ListContent export -> IRZY list blocks and TbRQ Slack Markdown.
//! Canonical children stay intact, including every old annotation control.
use super::super::list_body::ListBody;
use super::*;

struct Item {
    nodes: Vec<Node>,
    checked: bool,
}
enum Entry {
    Item(Item),
    List(Vec<Entry>),
}

pub(in crate::canvas::edit::annotation) fn capture(
    sections: &[Section],
    anchor: &Section,
    control: &str,
    quip_user: &str,
    slack_user: &str,
) -> Result<Value, Error> {
    let (rich, markdown) = render(&ListBody::capture(sections, anchor)?)?;
    super::capture_rendered(anchor, control, quip_user, slack_user, rich, markdown)
}
pub(in crate::canvas::edit::annotation) fn preview(
    sections: &[Section],
    anchor: &Section,
) -> Result<Vec<Value>, Error> {
    let (rich, _) = render(&ListBody::capture(sections, anchor)?)?;
    Ok(vec![rich])
}

pub(in crate::canvas::edit::annotation) fn capture_admitted(
    sections: &[Section],
    anchor: &Section,
    control: &str,
    quip_user: &str,
    slack_user: &str,
) -> Result<Value, Error> {
    let (rich, markdown) = render(&ListBody::capture_admitted(sections, anchor)?)?;
    super::capture_rendered(anchor, control, quip_user, slack_user, rich, markdown)
}
pub(in crate::canvas::edit::annotation) fn preview_admitted(
    sections: &[Section],
    anchor: &Section,
) -> Result<Vec<Value>, Error> {
    let (rich, _) = render(&ListBody::capture_admitted(sections, anchor)?)?;
    Ok(vec![rich])
}
pub(in crate::canvas::edit::annotation) fn capture_quotes(
    sections: &[Section],
    anchor: &Section,
    control: &str,
    quip_user: &str,
    slack_user: &str,
) -> Result<Value, Error> {
    let (rich, markdown) = render(&ListBody::capture_quotes(sections, anchor)?)?;
    super::capture_rendered(anchor, control, quip_user, slack_user, rich, markdown)
}
pub(in crate::canvas::edit::annotation) fn preview_quotes(
    sections: &[Section],
    anchor: &Section,
) -> Result<Vec<Value>, Error> {
    let (rich, _) = render(&ListBody::capture_quotes(sections, anchor)?)?;
    Ok(vec![rich])
}
pub(in crate::canvas::edit::annotation) fn capture_columns(
    sections: &[Section],
    anchor: &Section,
    control: &str,
    quip_user: &str,
    slack_user: &str,
) -> Result<Value, Error> {
    let (rich, markdown) = render(&ListBody::capture_columns(sections, anchor)?)?;
    super::capture_rendered(anchor, control, quip_user, slack_user, rich, markdown)
}
pub(in crate::canvas::edit::annotation) fn preview_columns(
    sections: &[Section],
    anchor: &Section,
) -> Result<Vec<Value>, Error> {
    let (rich, _) = render(&ListBody::capture_columns(sections, anchor)?)?;
    Ok(vec![rich])
}
pub(in crate::canvas::edit::annotation) fn preview_observed(
    sections: &[Section],
    anchor: &Section,
) -> Result<Vec<Value>, Error> {
    let (rich, _) = render(&ListBody::capture_observed(sections, anchor)?)?;
    Ok(vec![rich])
}

fn render(body: &ListBody<'_>) -> Result<(Value, String), Error> {
    let items = body
        .children
        .iter()
        .map(|child| {
            let nodes = if child.default_content {
                Vec::new()
            } else {
                let original = super::super::inline::original(child.section)?;
                let mut nodes = super::nodes(&original.draft)?;
                // contentOnly:true returns the original exported inline content,
                // before TextContent adds a top-level export-only trailing BR.
                if !matches!(nodes.pop(), Some(Node::Element("br", _, _))) {
                    return Err(Error::Unsupported);
                }
                nodes
            };
            // Keep an empty LI distinct from an empty list container.
            Ok((
                child.indent,
                Item {
                    nodes,
                    checked: child.checked,
                },
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    render_items(body.style, items)
}

fn render_items(style: u64, items: Vec<(usize, Item)>) -> Result<(Value, String), Error> {
    if !matches!(style, 5..=7) {
        return Err(Error::Unsupported);
    }
    if items.is_empty() {
        return Ok((json!({"type":"rich_text","elements":[]}), String::new()));
    }
    let mut stack: Vec<Vec<Entry>> = vec![Vec::new()];
    for (depth, item) in items {
        if depth > 64 {
            return Err(Error::Unsupported);
        }
        while stack.len() <= depth {
            stack.push(Vec::new());
        }
        while stack.len() > depth + 1 {
            let children = stack.pop().ok_or(Error::Unsupported)?;
            stack
                .last_mut()
                .ok_or(Error::Unsupported)?
                .push(Entry::List(children));
        }
        stack
            .last_mut()
            .ok_or(Error::Unsupported)?
            .push(Entry::Item(item));
    }
    while stack.len() > 1 {
        let children = stack.pop().ok_or(Error::Unsupported)?;
        stack
            .last_mut()
            .ok_or(Error::Unsupported)?
            .push(Entry::List(children));
    }
    let entries = stack.pop().ok_or(Error::Unsupported)?;
    let rich = rich_list(&entries, style, 0)?;
    let mut lines = Vec::new();
    markdown_list(&entries, style, 0, &mut lines)?;
    // TbRQ first appends LF to every LI except the last in document order.
    // Its later pg2g.append on all matched lists appends only to the first
    // receiver (the outer list), yielding these final two newlines.
    Ok((
        json!({"type":"rich_text","elements":rich}),
        format!("{}\n\n", lines.join("\n")),
    ))
}

fn rich_list(entries: &[Entry], style: u64, depth: usize) -> Result<Vec<Value>, Error> {
    let base = if style == 6 { "ordered" } else { "bullet" };
    let mut head: Vec<Value> = Vec::new();
    let mut tail = Vec::new();
    for entry in entries {
        match entry {
            Entry::List(children) => tail.extend(rich_list(children, style, depth + 1)?),
            Entry::Item(item) => {
                let kind = if style == 7 {
                    if item.checked { "checked" } else { "unchecked" }
                } else {
                    base
                };
                let mut leaves = Vec::new();
                rich_node(
                    &Node::Element("li", item.nodes.clone(), None),
                    &serde_json::Map::new(),
                    &mut leaves,
                )?;
                let section = json!({"type":"rich_text_section","elements":leaves});
                if !tail.is_empty() {
                    tail.push(json!({"type":"rich_text_list","elements":[section],"style":kind,"indent":depth}));
                } else if head.last().is_some_and(|last| last["style"] == kind) {
                    head.last_mut()
                        .and_then(|last| last["elements"].as_array_mut())
                        .ok_or(Error::Unsupported)?
                        .push(section);
                } else {
                    head.push(json!({"type":"rich_text_list","elements":[section],"style":kind,"indent":depth}));
                }
            }
        }
    }
    if head.is_empty() {
        head.push(json!({"type":"rich_text_list","elements":[],"style":base,"indent":depth}));
    }
    head.extend(tail);
    Ok(head)
}

fn markdown_list(
    entries: &[Entry],
    style: u64,
    depth: usize,
    lines: &mut Vec<String>,
) -> Result<(), Error> {
    let mut number = 0;
    for entry in entries {
        match entry {
            Entry::List(children) => markdown_list(children, style, depth + 1, lines)?,
            Entry::Item(item) => {
                number += 1;
                let mut nodes = item.nodes.clone();
                for (tag, marker) in [("b", "*"), ("i", "_"), ("del", "~"), ("code", "`")] {
                    apply(&mut nodes, tag, marker);
                }
                replace_breaks(&mut nodes, depth);
                links(&mut nodes)?;
                let marker = if style == 6 {
                    format!("{number}. ")
                } else {
                    "* ".into()
                };
                lines.push(format!(
                    "{}{marker}{}",
                    "    ".repeat(depth),
                    nodes.iter().map(|n| n.text(true)).collect::<String>()
                ));
            }
        }
    }
    Ok(())
}
fn replace_breaks(nodes: &mut [Node], depth: usize) {
    for node in nodes {
        if let Node::Element(tag, children, _) = node {
            if *tag == "br" {
                *node = Node::Text(format!("\n{}", "    ".repeat(depth + 1)));
            } else {
                replace_breaks(children, depth);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_list_export_rich_and_markdown_goldens_preserve_order_nesting_and_checks() {
        let cases: Vec<Value> = serde_json::from_str(include_str!("list_cases.json")).unwrap();
        assert_eq!(cases.len(), 7);
        for case in cases {
            let filter = case["filterChildId"].as_str();
            let items = case["children"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| filter.is_none_or(|id| v["id"] == id))
                .map(|child| {
                    let draft = crate::canvas::edit::text::TextDraft::from_rtml(
                        child["html"].as_str().unwrap(),
                    )
                    .unwrap();
                    let mut value = nodes(&draft).unwrap();
                    value.pop();
                    (
                        child["indent"].as_u64().unwrap() as usize,
                        Item {
                            nodes: value,
                            checked: child["checked"].as_bool().unwrap_or(false),
                        },
                    )
                })
                .collect();
            let (rich, markdown) = render_items(case["style"].as_u64().unwrap(), items).unwrap();
            assert_eq!(rich["elements"], case["rich"], "{}", case["name"]);
            assert_eq!(
                markdown,
                case["markdown"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}
