//! Retained TSF EDIT only; NORMAL markup conversion is not admitted here.
use super::{SeedUnavailable, regex::*};
pub(super) struct Pipeline {
    patterns: &'static Patterns,
    sequence: u64,
}
pub(super) type Saved = Vec<(Js, Js)>;
impl Pipeline {
    pub(super) fn save(&mut self, saved: &mut Saved, text: &[u16]) -> Js {
        if text.is_empty() {
            return vec![];
        }
        let prefix = if text.starts_with(&[10]) { "\n" } else { "" };
        let key = w(&format!("{prefix}\u{e022}-\u{e022}{}-", self.sequence));
        self.sequence += 1;
        saved.push((key.clone(), text.to_vec()));
        key
    }
    fn restore(&self, saved: &Saved, mut text: Js) -> Result<Js> {
        for (key, value) in saved.iter().rev() {
            text = replace(&text, &string(key)?, value);
        }
        Ok(text)
    }
}

/// Exact TSF EDIT mode: retain semantic markup, expose ordinary link labels,
/// leave literal formatting/emoji/newlines un-tokenized. No NORMAL fallback.
pub(super) fn tokenize_edit(text: &str) -> Result<Vec<String>> {
    if text.len() > 65_536 || text.contains('\u{e022}') {
        return Err(SeedUnavailable::Limit);
    }
    let p = Patterns::get()?;
    let mut pipeline = Pipeline {
        patterns: p,
        sequence: 0,
    };
    let mut saved = Saved::new();
    let text = p.map("line_norm", trim(&w(text)), |_| Ok(w("\n")))?;
    let text = p.map("markup", &text, |m| {
        let body = m.g(1);
        if body.first().is_some_and(|c| [35, 64, 33, 37].contains(c)) {
            return Ok(pipeline.save(&mut saved, m.g(0)));
        }
        let pipe = body.iter().position(|c| *c == 124);
        let url = replace(&body[..pipe.unwrap_or(body.len())], "\"", &w("&quot;"));
        if url.first() == Some(&60) {
            let body = replace(&replace(body, "<", &w("&lt;")), ">", &w("&gt;"));
            return Ok(pipeline.save(&mut saved, &cat(&[&w("&lt;"), &body, &w("&gt;")])));
        }
        let label = pipe
            .map(|i| &body[i + 1..])
            .filter(|s| !s.is_empty())
            .unwrap_or(&url);
        Ok(replace(
            &replace(trim(label), "<", &w("&lt;")),
            ">",
            &w("&gt;"),
        ))
    })?;
    let text = p.map("zero_edges", &text, |_| Ok(vec![]))?;
    let text = pipeline.restore(&saved, text)?;
    let mut at = 0;
    let mut tokens = vec![];
    while let Some(m) = p.find("markup", &text, at) {
        if m.range.start > at {
            tokens.push(string(&text[at..m.range.start])?);
        }
        tokens.push(string(&text[m.range.clone()])?);
        at = m.range.end;
    }
    if at < text.len() {
        tokens.push(string(&text[at..])?);
    }
    if tokens.len() > 32_768 {
        return Err(SeedUnavailable::Limit);
    }
    Ok(tokens)
}
