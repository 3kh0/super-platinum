//! Retained getCompactRtml (editor bundle, offset 264749): stable mark-density
//! ordering and a range tree, not independently wrapped run fragments.
use super::Error;
use automerge::{ScalarValue, marks::Mark};

#[derive(Clone)]
enum Node {
    Text {
        start: usize,
        units: Vec<u16>,
    },
    Element {
        start: usize,
        end: usize,
        tag: Option<String>,
        attributes: Option<String>,
        children: Vec<Node>,
    },
}

impl Node {
    fn start(&self) -> usize {
        match self {
            Self::Text { start, .. } | Self::Element { start, .. } => *start,
        }
    }
    fn end(&self) -> usize {
        match self {
            Self::Text { start, units } => start + units.len(),
            Self::Element { end, .. } => *end,
        }
    }
    fn element(tag: Option<String>, attributes: Option<String>, children: Vec<Self>) -> Self {
        let start = children.iter().map(Self::start).min().unwrap_or(0);
        let end = children.iter().map(Self::end).max().unwrap_or(0);
        Self::Element {
            start,
            end,
            tag,
            attributes,
            children,
        }
    }
    fn apply(&mut self, mark: &Mark, attributes: &Option<String>) {
        let Self::Element { children, .. } = self else {
            unreachable!()
        };
        // Source findNodeContaining_: descend before splitting siblings. This
        // preserves a spanning bold node around a shorter annotation.
        if let Some(child) = children.iter_mut().find(|child| {
            matches!(child, Self::Element { .. })
                && child.start() <= mark.start
                && mark.end <= child.end()
        }) {
            child.apply(mark, attributes);
            return;
        }
        let mut retained = Vec::new();
        let mut contained = Vec::new();
        for mut child in std::mem::take(children) {
            let start = child.start();
            let end = child.end();
            if end <= mark.start || mark.end <= start {
                retained.push(child);
            } else if mark.start <= start && end <= mark.end {
                contained.push(child);
            } else {
                match &mut child {
                    Self::Text { units, .. } => {
                        let from = start.max(mark.start);
                        let to = end.min(mark.end);
                        if start < from {
                            retained.push(Self::Text {
                                start,
                                units: units[..from - start].into(),
                            });
                        }
                        if to < end {
                            retained.push(Self::Text {
                                start: to,
                                units: units[to - start..].into(),
                            });
                        }
                        contained.push(Self::Text {
                            start: from,
                            units: units[from - start..to - start].into(),
                        });
                    }
                    Self::Element { .. } => {
                        let clipped = Mark::new(
                            mark.name().into(),
                            mark.value().clone(),
                            start.max(mark.start),
                            end.min(mark.end),
                        );
                        child.apply(&clipped, attributes);
                        retained.push(child);
                    }
                }
            }
        }
        if !contained.is_empty() {
            retained.push(Self::element(
                Some(mark.name().into()),
                attributes.clone(),
                contained,
            ));
        }
        *children = retained;
    }
    fn rtml(&mut self, out: &mut String) -> Result<(), Error> {
        match self {
            Self::Text { units, .. } => {
                out.push_str(&String::from_utf16(units).map_err(|_| Error::Unsupported)?);
            }
            Self::Element {
                tag,
                attributes,
                children,
                ..
            } => {
                if let Some(tag) = tag.as_ref() {
                    out.push('<');
                    out.push_str(tag);
                    if let Some(attributes) = attributes {
                        out.push(' ');
                        out.push_str(attributes);
                    }
                    out.push('>');
                }
                children.sort_by_key(Self::start);
                for child in children {
                    child.rtml(out)?;
                }
                if let Some(tag) = tag {
                    out.push_str("</");
                    out.push_str(tag);
                    out.push('>');
                }
            }
        }
        if out.len() > 128 * 1024 {
            return Err(Error::Unsupported);
        }
        Ok(())
    }
}

fn density(mark: &Mark) -> u8 {
    match mark.name() {
        "annotation" => 3,
        "a" => 2,
        _ => 0,
    }
}

fn attributes(mark: &Mark) -> Result<Option<String>, Error> {
    match (mark.name(), mark.value()) {
        ("b" | "i" | "u" | "del" | "code", ScalarValue::Boolean(true)) => Ok(None),
        ("annotation", ScalarValue::Str(id)) if super::existing::safe_id(id) => {
            Ok(Some(format!("id=\"{id}\"")))
        }
        ("a", ScalarValue::Str(value)) => {
            let raw: serde_json::Value =
                serde_json::from_str(value).map_err(|_| Error::Unsupported)?;
            let object = raw.as_object().ok_or(Error::Unsupported)?;
            let href = raw
                .get("href")
                .and_then(serde_json::Value::as_str)
                .ok_or(Error::Unsupported)?;
            // TextDraft admits href only. Do not invent the source's optional
            // nolink/slackFileId attributes or its malformed-JSON fallback.
            // The retained compactor emits an unescaped attribute: reject an
            // unsafe delimiter before it can change the parsed tree.
            if object.len() != 1
                || href
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '"' | '<' | '>'))
            {
                return Err(Error::Unsupported);
            }
            Ok(Some(format!("href=\"{href}\"")))
        }
        _ => Err(Error::Unsupported),
    }
}

pub(super) fn render(text: &str, marks: &[Mark]) -> Result<String, Error> {
    if text.len() > 64 * 1024 || marks.len() > 4097 {
        return Err(Error::Unsupported);
    }
    let units: Vec<_> = text.encode_utf16().collect();
    let boundary = |at: usize| {
        at <= units.len()
            && (at == 0
                || at == units.len()
                || !((0xd800..=0xdbff).contains(&units[at - 1])
                    && (0xdc00..=0xdfff).contains(&units[at])))
    };
    let mut active = Vec::new();
    for mark in marks {
        if !matches!(
            mark.name(),
            "b" | "i" | "u" | "del" | "code" | "a" | "annotation"
        ) {
            return Err(Error::Unsupported);
        }
        if mark.value() == &ScalarValue::Boolean(false) {
            continue;
        }
        if mark.start >= mark.end || !boundary(mark.start) || !boundary(mark.end) {
            return Err(Error::Unsupported);
        }
        active.push((mark, attributes(mark)?));
    }
    // The admitted typed model has at most one value for a mark name at any
    // offset. Refuse corrupted overlapping duplicates before building the
    // recursive tree, bounding its depth by the seven closed mark names.
    let mut spans = std::collections::BTreeMap::<&str, Vec<(usize, usize)>>::new();
    for (mark, _) in &active {
        spans
            .entry(mark.name())
            .or_default()
            .push((mark.start, mark.end));
    }
    for intervals in spans.values_mut() {
        intervals.sort_unstable();
        if intervals.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(Error::Unsupported);
        }
    }
    // sort_by_key is stable, matching actual source sortBy and preserving
    // equal-density format ordering from the actual Automerge mark list.
    active.sort_by_key(|(mark, _)| std::cmp::Reverse(density(mark)));
    let mut root = Node::element(None, None, vec![Node::Text { start: 0, units }]);
    for (mark, attributes) in active {
        root.apply(mark, &attributes);
    }
    let mut out = String::new();
    root.rtml(&mut out)?;
    Ok(out)
}
