//! A bounded, inert document tree for Slack's canvas HTML export.
//! No server HTML, styles, event attributes, or executable elements reach the renderer.
use scraper::{ElementRef, Html, Node, Selector};

pub mod edit;
pub mod wire;

pub const MAX_CANVAS_BYTES: usize = 2 * 1024 * 1024;
const MAX_NODES: usize = 20_000;
const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanvasKind {
    Group,
    Inline,
    Paragraph,
    Heading(u8),
    Bold,
    Italic,
    Strike,
    Underline,
    Code,
    Pre,
    Quote,
    List {
        ordered: bool,
        start: i32,
        checklist: bool,
    },
    Item {
        checked: Option<bool>,
    },
    Table,
    TableHead,
    TableBody,
    Row,
    Cell {
        heading: bool,
        columns: u8,
        rows: u8,
    },
    Link(String),
    Image {
        url: Option<String>,
        alt: String,
    },
    Break,
    Rule,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanvasNode {
    Text(String),
    Element {
        kind: CanvasKind,
        section: Option<String>,
        children: Vec<CanvasNode>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanvasDocument {
    pub nodes: Vec<CanvasNode>,
}

/// The authenticated export must come from Slack's file host, never a supplied
/// permalink or an arbitrary origin from file metadata.
pub fn content_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str() == Some("files.slack.com")
            && u.username().is_empty()
            && u.password().is_none()
            && u.port_or_known_default() == Some(443)
    })
}
pub fn link_url(value: &str) -> Option<String> {
    let u = url::Url::parse(value).ok()?;
    (matches!(u.scheme(), "https" | "http" | "mailto" | "slack")
        && u.username().is_empty()
        && u.password().is_none())
    .then(|| u.to_string())
}

impl CanvasDocument {
    pub fn parse(html: &str) -> Result<Self, &'static str> {
        if html.len() > MAX_CANVAS_BYTES {
            return Err("canvas_too_large");
        }
        let dom = Html::parse_document(html);
        let selector = Selector::parse(".quip-canvas-content").expect("constant selector");
        let root = dom
            .select(&selector)
            .next()
            .ok_or("canvas_content_missing")?;
        let mut count = 0;
        Ok(Self {
            nodes: children(root, 0, false, &mut count)?,
        })
    }
}
fn children(
    el: ElementRef<'_>,
    depth: usize,
    checklist: bool,
    count: &mut usize,
) -> Result<Vec<CanvasNode>, &'static str> {
    if depth > MAX_DEPTH {
        return Err("canvas_too_deep");
    }
    let mut nodes = Vec::new();
    for child in el.children() {
        *count += 1;
        if *count > MAX_NODES {
            return Err("canvas_too_many_nodes");
        }
        match child.value() {
            Node::Text(text) => nodes.push(CanvasNode::Text(text.to_string())),
            Node::Element(_) => {
                if let Some(el) = ElementRef::wrap(child) {
                    let tag = el.value().name();
                    if matches!(
                        tag,
                        "script"
                            | "style"
                            | "iframe"
                            | "object"
                            | "embed"
                            | "svg"
                            | "math"
                            | "template"
                            | "noscript"
                            | "form"
                            | "input"
                            | "button"
                            | "select"
                            | "textarea"
                    ) {
                        continue;
                    }
                    let attr = |name| el.value().attr(name);
                    let checklist = if tag == "div" {
                        attr("data-section-style")
                            .map(|style| style == "7")
                            .unwrap_or(checklist)
                    } else {
                        checklist
                    };
                    let bounded = |name| {
                        attr(name)
                            .and_then(|v| v.parse::<u8>().ok())
                            .unwrap_or(1)
                            .clamp(1, 32)
                    };
                    let kind = match tag {
                        "p" => CanvasKind::Paragraph,
                        "h1" => CanvasKind::Heading(1),
                        "h2" => CanvasKind::Heading(2),
                        "h3" => CanvasKind::Heading(3),
                        "h4" | "h5" | "h6" => CanvasKind::Heading(4),
                        "b" | "strong" => CanvasKind::Bold,
                        "i" | "em" => CanvasKind::Italic,
                        "s" | "strike" | "del" => CanvasKind::Strike,
                        "u" => CanvasKind::Underline,
                        "code" => CanvasKind::Code,
                        "pre" => CanvasKind::Pre,
                        "blockquote" => CanvasKind::Quote,
                        "ul" | "ol" => CanvasKind::List {
                            ordered: tag == "ol",
                            start: attr("start").and_then(|v| v.parse().ok()).unwrap_or(1),
                            checklist,
                        },
                        "li" => CanvasKind::Item {
                            checked: checklist
                                .then(|| el.value().classes().any(|c| c == "checked")),
                        },
                        "table" => CanvasKind::Table,
                        "thead" => CanvasKind::TableHead,
                        "tbody" | "tfoot" => CanvasKind::TableBody,
                        "tr" => CanvasKind::Row,
                        "td" | "th" => CanvasKind::Cell {
                            heading: tag == "th",
                            columns: bounded("colspan"),
                            rows: bounded("rowspan"),
                        },
                        "a" => attr("href")
                            .and_then(link_url)
                            .map(CanvasKind::Link)
                            .unwrap_or(CanvasKind::Inline),
                        "img" => CanvasKind::Image {
                            url: attr("src")
                                .and_then(link_url)
                                .filter(|u| u.starts_with("https://") || u.starts_with("http://")),
                            alt: attr("alt").unwrap_or("Canvas image").into(),
                        },
                        "br" => CanvasKind::Break,
                        "hr" => CanvasKind::Rule,
                        "div" | "section" => CanvasKind::Group,
                        _ => CanvasKind::Inline,
                    };
                    let section = attr("id").filter(|id| id.len() <= 256).map(str::to_owned);
                    let nested = children(el, depth + 1, checklist, count)?;
                    nodes.push(CanvasNode::Element {
                        kind,
                        section,
                        children: nested,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tree(body: &str) -> CanvasDocument {
        CanvasDocument::parse(&format!("<div class='quip-canvas-content'>{body}</div>")).unwrap()
    }
    #[test]
    fn preserves_slack_export_structure_and_entities() {
        let doc = tree(
            "<h2 id='section'>A &amp; B 🦊</h2><div data-section-style='7'><ul><li>Pending</li><li class='checked'>Done</li></ul></div><table><tr><th colspan='2'>Title</th><td><b>Bold</b></td></tr></table>",
        );
        let debug = format!("{doc:?}");
        for part in [
            "Heading(2)",
            "A & B 🦊",
            "section",
            "checked: Some(false)",
            "checked: Some(true)",
            "columns: 2",
            "Bold",
        ] {
            assert!(debug.contains(part), "{part}");
        }
    }
    #[test]
    fn strips_executable_content_and_unsafe_links() {
        let doc = tree(
            "<script>BAD</script><iframe>BAD</iframe><svg><text>BAD</text></svg><style>BAD</style><a href='javascript:BAD' onclick='BAD'>Keep</a><p style='display:none'>Visible</p><img src='data:text/html,BAD' onerror='BAD' alt='Fallback'>",
        );
        let debug = format!("{doc:?}");
        assert!(!debug.contains("BAD"));
        assert!(debug.contains("Keep"));
        assert!(debug.contains("Visible"));
        assert!(debug.contains("Fallback"));
    }
    #[test]
    fn content_origin_is_exact_and_links_are_typed() {
        assert!(content_url("https://files.slack.com/files-pri/T-F/doc"));
        for u in [
            "http://files.slack.com/a",
            "https://files.slack.com.evil.org/a",
            "https://files.slack.com@evil.org/a",
            "https://user@files.slack.com/a",
            "https://files.slack.com:444/a",
            "file:///tmp/a",
        ] {
            assert!(!content_url(u));
        }
        assert!(link_url("https://example.com/path").is_some());
        for u in [
            "javascript:alert(1)",
            "data:text/html,test",
            "file:///tmp/a",
            "//evil.org",
        ] {
            assert!(link_url(u).is_none());
        }
    }
    #[test]
    fn rejects_login_pages_and_bounded_content_without_silent_truncation() {
        assert_eq!(
            CanvasDocument::parse("<html>Sign in</html>"),
            Err("canvas_content_missing")
        );
        assert_eq!(
            CanvasDocument::parse(&"x".repeat(MAX_CANVAS_BYTES + 1)),
            Err("canvas_too_large")
        );
        let deep = format!(
            "<div class='quip-canvas-content'>{}x{}</div>",
            "<div>".repeat(70),
            "</div>".repeat(70)
        );
        assert_eq!(CanvasDocument::parse(&deep), Err("canvas_too_deep"));
        let wide = format!(
            "<div class='quip-canvas-content'>{}</div>",
            "<br>".repeat(MAX_NODES + 1)
        );
        assert_eq!(CanvasDocument::parse(&wide), Err("canvas_too_many_nodes"));
    }
}
