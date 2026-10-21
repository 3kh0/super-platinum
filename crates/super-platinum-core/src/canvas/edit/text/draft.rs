use scraper::{ElementRef, Html, Node};

use super::Error;

pub(super) const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_RUNS: usize = 4096;

#[derive(Clone, Default, PartialEq, Eq)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TextRun {
    pub text: String,
    pub style: TextStyle,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct TextDraft {
    pub runs: Vec<TextRun>,
}

/// Source GoiO marks refer to escaped text, not visible Unicode character
/// offsets. In particular '&' occupies five units and a soft break four.
pub(in crate::canvas::edit) struct Model {
    pub text: String,
    pub marks: Vec<automerge::marks::Mark>,
}

impl TextDraft {
    pub fn from_rtml(source: &str) -> Result<Self, Error> {
        if source.len() > MAX_TEXT_BYTES || source.contains('\0') {
            return Err(Error::Unsupported);
        }
        let parsed = Html::parse_fragment(source);
        // HTML repair can discard unknown source bytes before tree traversal.
        if !parsed.errors.is_empty() {
            return Err(Error::Unsupported);
        }
        let mut result = Self::default();
        result.walk(parsed.root_element(), TextStyle::default(), 0)?;
        result.validate()?;
        Ok(result)
    }

    fn walk(
        &mut self,
        parent: ElementRef<'_>,
        inherited: TextStyle,
        depth: usize,
    ) -> Result<(), Error> {
        if depth > 32 {
            return Err(Error::Unsupported);
        }
        for child in parent.children() {
            match child.value() {
                Node::Text(text) => self.push(normalize(text), inherited.clone())?,
                Node::Element(element) => {
                    let mut style = inherited.clone();
                    let name = element.name();
                    if name == "a" {
                        if style.link.is_some() || element.attrs().any(|(key, _)| key != "href") {
                            return Err(Error::Unsupported);
                        }
                        style.link = Some(element.attr("href").unwrap_or("").to_owned());
                    } else {
                        if element.attrs().next().is_some() {
                            return Err(Error::Unsupported);
                        }
                        let flag = match name {
                            "b" => Some(&mut style.bold),
                            "i" => Some(&mut style.italic),
                            "u" => Some(&mut style.underline),
                            "del" => Some(&mut style.strike),
                            "code" => Some(&mut style.code),
                            "br" => None,
                            _ => return Err(Error::Unsupported),
                        };
                        if let Some(flag) = flag {
                            // GoiO permits only one open mark of each name.
                            if *flag {
                                return Err(Error::Unsupported);
                            }
                            *flag = true;
                        }
                    }
                    if name != "br"
                        && ElementRef::wrap(child)
                            .ok_or(Error::Unsupported)?
                            .text()
                            .all(str::is_empty)
                    {
                        // YLF4 ignores formatting containers with no textContent.
                        style = inherited.clone();
                    }
                    if name == "br" {
                        self.push("\n".into(), inherited.clone())?;
                    } else {
                        self.walk(
                            ElementRef::wrap(child).ok_or(Error::Unsupported)?,
                            style,
                            depth + 1,
                        )?;
                    }
                }
                _ => return Err(Error::Unsupported),
            }
        }
        Ok(())
    }

    fn push(&mut self, text: String, style: TextStyle) -> Result<(), Error> {
        if text.is_empty() {
            return Ok(());
        }
        if let Some(previous) = self.runs.last_mut().filter(|r| r.style == style) {
            previous.text.push_str(&text);
        } else {
            if self.runs.len() >= MAX_RUNS {
                return Err(Error::Unsupported);
            }
            self.runs.push(TextRun { text, style });
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.runs.len() > MAX_RUNS
            || self
                .runs
                .iter()
                .map(|r| r.text.len() + r.style.link.as_ref().map_or(0, String::len))
                .sum::<usize>()
                > MAX_TEXT_BYTES
            || self.runs.iter().any(|r| {
                (!r.text.is_empty()
                    && r.text.chars().all(|c| c == '\n')
                    && r.style != TextStyle::default())
                    || r.text.chars().any(|c| c.is_control() && c != '\n')
                    || r.style
                        .link
                        .as_ref()
                        .is_some_and(|url| url.chars().any(char::is_control))
            })
        {
            return Err(Error::Unsupported);
        }
        Ok(())
    }

    pub fn rtml(&self) -> Result<String, Error> {
        self.validate()?;
        let mut result = String::new();
        for run in &self.runs {
            let mut closes = Vec::new();
            for (enabled, tag) in [
                (run.style.bold, "b"),
                (run.style.italic, "i"),
                (run.style.underline, "u"),
                (run.style.strike, "del"),
                (run.style.code, "code"),
            ] {
                if enabled {
                    result.push_str(&format!("<{tag}>"));
                    closes.push(tag);
                }
            }
            if let Some(href) = &run.style.link {
                result.push_str(&format!(
                    "<a href=\"{}\">",
                    escape(href).replace('"', "&quot;")
                ));
                closes.push("a");
            }
            result.push_str(&escape(&run.text).replace('\n', "<br>"));
            for tag in closes.into_iter().rev() {
                result.push_str(&format!("</{tag}>"));
            }
        }
        if result.len() > MAX_TEXT_BYTES {
            return Err(Error::Unsupported);
        }
        Ok(result)
    }

    pub(in crate::canvas::edit) fn model(&self) -> Result<Model, Error> {
        self.validate()?;
        let mut text = String::new();
        let mut marks = Vec::new();
        let mut units = 0;
        for run in &self.runs {
            let value = escape(&run.text).replace('\n', "<br>");
            let end = units + value.encode_utf16().count();
            for (enabled, name) in [
                (run.style.bold, "b"),
                (run.style.italic, "i"),
                (run.style.underline, "u"),
                (run.style.strike, "del"),
                (run.style.code, "code"),
            ] {
                if enabled && end > units {
                    marks.push(automerge::marks::Mark::new(name.into(), true, units, end));
                }
            }
            if let Some(href) = &run.style.link {
                if end > units {
                    marks.push(automerge::marks::Mark::new(
                        "a".into(),
                        serde_json::json!({"href":href}).to_string(),
                        units,
                        end,
                    ));
                }
            }
            text.push_str(&value);
            units = end;
        }
        if text.len() > MAX_TEXT_BYTES {
            return Err(Error::Unsupported);
        }
        Ok(Model { text, marks })
    }
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\t' | '\n' | '\r' => Some(' '),
            '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => None,
            _ => Some(c),
        })
        .collect()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
