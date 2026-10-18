//! The section-highlight branch of getHighlightControlSnippet. This does not
//! use the body HTML export as write authority: the exact canonical section is
//! already captured and revalidated by AnnotationIntent.
use super::*;
use serde_json::{Value, json};
mod inline;
mod list;
pub(super) use list::{
    capture as list_capture, capture_admitted as list_capture_admitted,
    capture_columns as list_capture_columns, capture_quotes as list_capture_quotes,
    preview as list_preview, preview_admitted as list_preview_admitted,
    preview_columns as list_preview_columns, preview_observed as list_preview_observed,
    preview_quotes as list_preview_quotes,
};
mod styled;
mod urls;
pub(super) use inline::capture as capture_inline;

pub(super) fn capture(
    section: &Section,
    control: &str,
    quip_user: &str,
    slack_user: &str,
    _workspace: &str,
) -> Result<Value, Error> {
    let draft = super::super::text::canonical_draft(section)?;
    capture_nodes(section, control, quip_user, slack_user, nodes(&draft)?)
}

pub(super) fn capture_styled(
    section: &Section,
    control: &str,
    quip_user: &str,
    slack_user: &str,
) -> Result<Value, Error> {
    let draft = super::super::text::canonical_draft(section)?;
    capture_nodes(
        section,
        control,
        quip_user,
        slack_user,
        styled::nodes(section, &draft)?,
    )
}

pub(super) fn preview(section: &Section) -> Result<Vec<Value>, Error> {
    let draft = super::super::text::canonical_draft(section)?;
    Ok(vec![rich_nodes(&styled::nodes(section, &draft)?)?])
}

pub(super) fn capture_whole(
    section: &Section,
    control: &str,
    quip_user: &str,
    slack_user: &str,
) -> Result<Value, Error> {
    let original = super::inline::original(section)?;
    capture_nodes(
        section,
        control,
        quip_user,
        slack_user,
        styled::nodes(section, &original.draft)?,
    )
}

pub(super) fn preview_whole(section: &Section) -> Result<Vec<Value>, Error> {
    let original = super::inline::original(section)?;
    Ok(vec![rich_nodes(&styled::nodes(section, &original.draft)?)?])
}

fn capture_nodes(
    section: &Section,
    control: &str,
    quip_user: &str,
    slack_user: &str,
    nodes: Vec<Node>,
) -> Result<Value, Error> {
    capture_rendered(
        section,
        control,
        quip_user,
        slack_user,
        rich_nodes(&nodes)?,
        markdown_nodes(nodes)?,
    )
}

pub(in crate::canvas::edit::annotation) fn capture_rendered(
    section: &Section,
    control: &str,
    quip_user: &str,
    slack_user: &str,
    rich: Value,
    markdown: String,
) -> Result<Value, Error> {
    let mut users = strings(section.raw(), 31)?;
    if users.len() > 256
        || users
            .iter()
            .any(|u| u.is_empty() || u.len() > 256 || u.chars().any(char::is_control))
    {
        return Err(Error::Unsupported);
    }
    // effectiveAuthorId -> mostRecentEditor -> current Quip user. The mapping
    // store may omit Slack IDs; never fabricate one from a different namespace.
    let explicit_author = section.raw().string(30)?;
    if explicit_author
        .is_some_and(|id| id.is_empty() || id.len() > 256 || id.chars().any(char::is_control))
    {
        return Err(Error::Unsupported);
    }
    let author = explicit_author
        .or_else(|| users.first().map(String::as_str))
        .unwrap_or(quip_user)
        .to_owned();
    if !users.contains(&author) {
        users.push(author.clone());
    }
    let most_recent = users.first().cloned();
    let users = users
        .into_iter()
        .map(|id| {
            let mut v = json!({"quipUserId":id,"isAuthor":id==author,"isRecentEditor":id!=author,
            "isMostRecentEditor":Some(&id)==most_recent.as_ref(),"isMentioned":false});
            if id == quip_user {
                v["slackUserId"] = json!(slack_user);
                // The initialization workspace is not the Quip company store's
                // top-level team mapping (enterprise may differ). Do not invent it.
            }
            v
        })
        .collect::<Vec<_>>();
    let mut value = json!({"richText":serde_json::to_string(&json!({"blocks":[rich]})).map_err(|_|Error::Unsupported)?,
        "mrkdwn":markdown,"sectionUsers":users,"isArchived":false,"controlId":control});
    let created = section.raw().uint(26)?;
    let edited = section.raw().uint(32)?.or(created);
    for (key, number) in [("createdUsec", created), ("lastEditedUsec", edited)] {
        if let Some(number) = number {
            if number > 9_007_199_254_740_991 {
                return Err(Error::Unsupported);
            }
            value[key] = json!(number);
        }
    }
    if serde_json::to_vec(&value)
        .map_err(|_| Error::Unsupported)?
        .len()
        > 192 * 1024
    {
        return Err(Error::Unsupported);
    }
    Ok(value)
}

#[derive(Clone)]
enum Node {
    Text(String),
    Element(&'static str, Vec<Node>, Option<String>),
}
impl Node {
    fn text(&self, breaks: bool) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Element("br", _, _) => {
                if breaks {
                    "\n".into()
                } else {
                    String::new()
                }
            }
            Self::Element(_, children, _) => children.iter().map(|n| n.text(breaks)).collect(),
        }
    }
}
fn space(c: char) -> bool {
    // ECMAScript trim, including FEFF (Rust's char::is_whitespace differs).
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn apply(nodes: &mut [Node], tag: &str, mark: &str) {
    for i in 0..nodes.len() {
        let previous = i
            .checked_sub(1)
            .map(|j| nodes[j].text(false))
            .unwrap_or_default();
        let next = nodes.get(i + 1).map(|n| n.text(false)).unwrap_or_default();
        if let Node::Element(name, children, _) = &mut nodes[i] {
            if *name == tag {
                if tag == "code" {
                    children.insert(0, Node::Text(mark.into()));
                    children.push(Node::Text(mark.into()));
                } else {
                    let mut leading = false;
                    let mut trailing = false;
                    let same = children.len() == 1;
                    if let Some(Node::Text(s)) = children.first_mut() {
                        leading = s.starts_with(space);
                        if same {
                            trailing = s.ends_with(space);
                            *s = s.trim_matches(space).into();
                        } else {
                            *s = s.trim_start_matches(space).into();
                        }
                    }
                    if !same && let Some(Node::Text(s)) = children.last_mut() {
                        trailing = s.ends_with(space);
                        *s = s.trim_end_matches(space).into();
                    }
                    let before = if leading && !previous.ends_with(space) {
                        " "
                    } else {
                        ""
                    };
                    let after = if trailing && !next.starts_with(space) {
                        " "
                    } else {
                        ""
                    };
                    children.insert(0, Node::Text(format!("{before}{mark}")));
                    children.push(Node::Text(format!("{mark}{after}")));
                }
            }
            apply(children, tag, mark);
        }
    }
}
fn links(nodes: &mut [Node]) -> Result<(), Error> {
    for node in nodes {
        if let Node::Element(tag, children, url) = node {
            if *tag == "a" {
                let url = url.as_ref().ok_or(Error::Unsupported)?;
                // Relative links require the real export base URL. Refuse them
                // before an anchor write rather than guess its workspace host.
                let parsed = url::Url::parse(url).map_err(|_| Error::Unsupported)?;
                if !matches!(parsed.scheme(), "http" | "https" | "mailto" | "tel") {
                    return Err(Error::Unsupported);
                }
                let url = parsed.as_str();
                let label = children.iter().map(|n| n.text(false)).collect::<String>();
                let label = label.trim_matches(space);
                let value = if label == url {
                    label.into()
                } else if let Some((kind, id)) = urls::slack_identity(url) {
                    format!("<{}{id}>", if kind == "user" { "@" } else { "#" })
                } else {
                    format!("<{url}|{label}>")
                };
                *children = vec![Node::Text(value)];
            } else {
                links(children)?;
            }
        }
    }
    Ok(())
}
fn nodes(draft: &super::super::text::TextDraft) -> Result<Vec<Node>, Error> {
    draft.validate()?;
    let mut nodes = Vec::new();
    for run in &draft.runs {
        let mut children = Vec::new();
        for (i, line) in run.text.split('\n').enumerate() {
            if i > 0 {
                children.push(Node::Element("br", vec![], None));
            }
            if !line.is_empty() {
                children.push(Node::Text(line.into()));
            }
        }
        if let Some(url) = &run.style.link {
            children = vec![Node::Element("a", children, Some(url.clone()))];
        }
        for (enabled, tag) in [
            (run.style.code, "code"),
            (run.style.strike, "del"),
            (run.style.italic, "i"),
            (run.style.bold, "b"),
        ] {
            if enabled {
                children = vec![Node::Element(tag, children, None)];
            }
        }
        nodes.extend(children);
    }
    // TextContent.exportToHtml(EXTERNAL_PASTE), ordinary top-level paragraph:
    // no enclosing <p>, and a mandatory trailing <br> when skipTrailingBreak
    // is absent. This is source snippet presentation, not canonical write data.
    nodes.push(Node::Element("br", vec![], None));
    Ok(nodes)
}
fn markdown(draft: &super::super::text::TextDraft) -> Result<String, Error> {
    markdown_nodes(nodes(draft)?)
}
fn markdown_nodes(mut nodes: Vec<Node>) -> Result<String, Error> {
    // Exact pass order used by retained TbRQ.convertDomToMarkdown(SLACK).
    for (tag, marker) in [("b", "*"), ("i", "_"), ("del", "~"), ("code", "`")] {
        apply(&mut nodes, tag, marker);
    }
    styled::markdown_blocks(&mut nodes);
    links(&mut nodes)?;
    Ok(nodes.iter().map(|n| n.text(true)).collect())
}

fn rich_text(draft: &super::super::text::TextDraft) -> Result<Value, Error> {
    rich_nodes(&nodes(draft)?)
}
fn rich_nodes(nodes: &[Node]) -> Result<Value, Error> {
    let mut sections = Vec::new();
    let mut leaves = Vec::new();
    // normalizeSectionsDom splits only top-level breaks; a break nested in a
    // style becomes an actual newline text element in that styled section.
    for node in nodes {
        if matches!(node, Node::Element("br", _, _)) {
            if !leaves.is_empty() {
                sections.push(
                    json!({"type":"rich_text_section","elements":std::mem::take(&mut leaves)}),
                );
            }
        } else if let Some(block) = styled::rich_block(node)? {
            if !leaves.is_empty() {
                sections.push(
                    json!({"type":"rich_text_section","elements":std::mem::take(&mut leaves)}),
                );
            }
            sections.push(block);
        } else {
            rich_node(node, &serde_json::Map::new(), &mut leaves)?;
        }
    }
    if !leaves.is_empty() {
        sections.push(json!({"type":"rich_text_section","elements":leaves}));
    }
    Ok(json!({"type":"rich_text","elements":sections}))
}
fn rich_node(
    node: &Node,
    inherited: &serde_json::Map<String, Value>,
    out: &mut Vec<Value>,
) -> Result<(), Error> {
    let mut style = inherited.clone();
    let mut value = match node {
        Node::Text(text) => json!({"type":"text","text":text}),
        Node::Element("br", _, _) => json!({"type":"text","text":"\n"}),
        Node::Element("a", children, Some(href)) => {
            let parsed = url::Url::parse(href).map_err(|_| Error::Unsupported)?;
            if !matches!(parsed.scheme(), "http" | "https" | "mailto" | "tel") {
                return Err(Error::Unsupported);
            }
            let label = children.iter().map(|n| n.text(false)).collect::<String>();
            if let Some((kind, id)) = urls::slack_identity(parsed.as_str()) {
                if kind == "user" {
                    json!({"type":"user","user_id":id})
                } else {
                    json!({"type":"channel","channel_id":id})
                }
            } else {
                // IRZY.typedHttpUrl keeps an original absolute HTTP URL when
                // the browser resolves it to the same URL, not its display label.
                let url = if matches!(parsed.scheme(), "http" | "https") {
                    href.as_str()
                } else {
                    parsed.as_str()
                };
                json!({"type":"link","text":label,"url":url})
            }
        }
        Node::Element(tag, children, _) => {
            if let Some(name) = match *tag {
                "b" => Some("bold"),
                "i" => Some("italic"),
                "del" => Some("strike"),
                "code" => Some("code"),
                _ => None,
            } {
                style.insert(name.into(), json!(true));
            }
            if !children.is_empty() {
                for child in children {
                    rich_node(child, &style, out)?;
                }
                return Ok(());
            }
            json!({"type":"text","text":""})
        }
    };
    if !style.is_empty() {
        value["style"] = Value::Object(style);
    }
    out.push(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::edit::text::TextDraft;
    #[test]
    fn snippet_rich_text_uses_export_breaks_and_typed_slack_links() {
        let d = TextDraft::from_rtml("<b>x<br>y</b>").unwrap();
        assert_eq!(
            rich_text(&d).unwrap(),
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":"x","style":{"bold":true}},
            {"type":"text","text":"\n","style":{"bold":true}},
            {"type":"text","text":"y","style":{"bold":true}}]}]})
        );
        let d = TextDraft::from_rtml("x<br>y").unwrap();
        assert_eq!(
            rich_text(&d).unwrap()["elements"].as_array().unwrap().len(),
            2
        );
        for (url, kind, key, id, markdown_expected) in [
            (
                "https://fixture.slack.com/team/U12345678",
                "user",
                "user_id",
                "U12345678",
                "<@U12345678>\n",
            ),
            (
                "https://fixture.slack.com/archives/C12345678",
                "channel",
                "channel_id",
                "C12345678",
                "<#C12345678>\n",
            ),
        ] {
            let d = TextDraft::from_rtml(&format!("<a href=\"{url}\">label</a>")).unwrap();
            let element = rich_text(&d).unwrap()["elements"][0]["elements"][0].clone();
            assert_eq!(element["type"], kind);
            assert_eq!(element[key], id);
            assert!(element.get("text").is_none());
            assert_eq!(markdown(&d).unwrap(), markdown_expected);
        }
    }
    #[test]
    fn known_section_markers_keep_rich_text_and_source_pass_order() {
        for (input, expected) in [
            ("Hi <b> bold </b>", "Hi *bold* \n"),
            ("<b><i> both </i></b>", "* _both_ *\n"),
            ("<code>x<br>y</code>", "`x\ny`\n"),
            (
                "<a href=\"https://example.invalid/\"><b>label</b></a>",
                "*<https://example.invalid/|label>*\n",
            ),
            ("<del>x</del> \u{1f600}", "~x~ \u{1f600}\n"),
        ] {
            assert_eq!(
                markdown(&TextDraft::from_rtml(input).unwrap()).unwrap(),
                expected
            );
        }
        assert!(
            markdown(&TextDraft::from_rtml("<a href=\"/relative\">link</a>").unwrap()).is_err()
        );
    }
}
