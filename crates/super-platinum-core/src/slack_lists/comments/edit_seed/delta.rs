//! W05w's inline/block state machine, retaining semantic atoms and labels.
use super::*;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}
impl Style {
    pub fn apply(&self, mut v: Value) -> Value {
        let mut style = serde_json::Map::new();
        for (key, on) in [
            ("bold", self.bold),
            ("italic", self.italic),
            ("strike", self.strike),
            ("code", self.code),
        ] {
            if on {
                style.insert(key.into(), true.into());
            }
        }
        if !style.is_empty() {
            v["style"] = Value::Object(style)
        }
        v
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Block {
    Section,
    Quote,
    Pre,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Insert {
    Text(String),
    Atom(Value, String),
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Op {
    pub insert: Insert,
    pub style: Style,
    pub block: Block,
}
fn push(ops: &mut Vec<Op>, insert: Insert, style: &Style, block: Block) {
    if let Insert::Text(text) = &insert {
        if text.is_empty() {
            return;
        }
        if let Some(Op {
            insert: Insert::Text(previous),
            style: s,
            block: b,
        }) = ops.last_mut()
        {
            if s == style && *b == block {
                previous.push_str(text);
                return;
            }
        }
    }
    ops.push(Op {
        insert,
        style: style.clone(),
        block,
    });
}
fn br(ops: &mut Vec<Op>, block: Block) {
    push(ops, Insert::Text("\n".into()), &Style::default(), block);
}
fn line(t: &str) -> bool {
    matches!(t, "<LINE:BREAK>" | "<PARA:BREAK>")
}
fn end(t: &str) -> bool {
    matches!(t, "<PRE:END>" | "<QUOTE:END>")
}
fn label<'a>(
    map: &'a BTreeMap<String, String>,
    id: &str,
    prefix: char,
    missing: SeedUnavailable,
) -> Result<&'a str, SeedUnavailable> {
    let empty_group = matches!(&missing, SeedUnavailable::MissingUsergroup(_));
    let s = map.get(id).ok_or(missing)?;
    if id.is_empty() || !(s.starts_with(prefix) || empty_group && s.is_empty()) {
        return Err(SeedUnavailable::InvalidObservation);
    }
    Ok(s)
}
pub(super) fn unescape(text: &str) -> String {
    // lodash unescape is one-pass; decoding &amp;lt; does not recursively decode.
    let mut out = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some((key, value)) = [
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
            ("&#39;", "'"),
        ]
        .iter()
        .find(|(k, _)| rest.starts_with(k))
        {
            out.push_str(value);
            rest = &rest[key.len()..];
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}
pub(super) fn convert(
    tokens: &[String],
    obs: &SeedObservations,
) -> Result<Vec<Op>, SeedUnavailable> {
    if tokens.is_empty() {
        return Ok(vec![]);
    }
    let emoji_text = obs
        .emoji_as_text
        .ok_or(SeedUnavailable::UnknownEmojiPreference)?;
    let mut ops = vec![];
    let mut style = Style::default();
    let mut block = Block::Section;
    let mut end_next = false;
    for (i, t) in tokens.iter().enumerate() {
        let pre = block == Block::Pre;
        match t.as_str() {
            "<B:START>" => {
                if !pre {
                    style.bold = true
                }
            }
            "<B:END>" => style.bold = false,
            "<I:START>" => {
                if !pre {
                    style.italic = true
                }
            }
            "<I:END>" => style.italic = false,
            "<STRIKE:START>" => {
                if !pre {
                    style.strike = true
                }
            }
            "<STRIKE:END>" => style.strike = false,
            "<CODE:START>" => {
                if !pre {
                    style.code = true
                }
            }
            "<CODE:END>" => style.code = false,
            "<LINK:END>" => style.link = None,
            "<LINE:BREAK>" | "<PARA:BREAK>" => {
                br(&mut ops, block);
                if t == "<PARA:BREAK>" {
                    br(&mut ops, if end_next { Block::Section } else { block });
                }
                if end_next {
                    end_next = false;
                    block = Block::Section;
                }
                if tokens.get(i + 1).is_some_and(|t| end(t)) {
                    block = Block::Section;
                }
            }
            "<PRE:START>" | "<QUOTE:START>" | "<LONGQUOTE:PREFIX>" => {
                if i > 0 && !line(&tokens[i - 1]) && !end(&tokens[i - 1]) {
                    br(&mut ops, block);
                }
                block = if t == "<PRE:START>" {
                    Block::Pre
                } else {
                    Block::Quote
                };
            }
            "<PRE:END>" | "<QUOTE:END>" => {
                if i > 0 && line(&tokens[i - 1]) || tokens.get(i + 1).is_some_and(|t| line(t)) {
                    if i + 1 < tokens.len() {
                        end_next = true;
                    }
                } else {
                    br(&mut ops, block);
                    block = Block::Section;
                }
            }
            "<SPACE:HARD>" => push(&mut ops, Insert::Text(" ".into()), &style, Block::Section),
            "<QUOTE:PREFIX>" => {}
            _ if t.starts_with("<LINK:START ") => {
                if !pre {
                    style.link = Some(t[12..t.len() - 1].into());
                }
            }
            _ if t.starts_with("<HEX:BLOCK ") => {} // TSF decoration; text converter emits the color atom.
            _ if t.starts_with("<EMOJI:COLONS ") => {
                let code = &t[14..t.len() - 1];
                if emoji_text || pre {
                    push(&mut ops, Insert::Text(code.into()), &style, Block::Section);
                } else {
                    let e = obs
                        .emoji
                        .get(code)
                        .ok_or_else(|| SeedUnavailable::MissingEmoji(code.into()))?;
                    if e.name.is_empty()
                        || e.name.contains(':')
                        || e.skin_tone.is_some_and(|s| !(2..=6).contains(&s))
                    {
                        return Err(SeedUnavailable::InvalidObservation);
                    }
                    let mut atom = json!({"type":"emoji","name":e.name});
                    if let Some(unicode) = &e.unicode {
                        atom["unicode"] = unicode.clone().into();
                    }
                    if let Some(tone) = e.skin_tone {
                        atom["skin_tone"] = tone.into();
                    }
                    push(
                        &mut ops,
                        Insert::Atom(atom, code.into()),
                        &style,
                        Block::Section,
                    );
                }
            }
            _ if t.starts_with("<@") => {
                let id = &t[2..t.len() - 1];
                let name = label(&obs.users, id, '@', SeedUnavailable::MissingUser(id.into()))?;
                let atom = if pre {
                    Insert::Text(name.into())
                } else {
                    Insert::Atom(json!({"type":"user","user_id":id}), name.into())
                };
                push(&mut ops, atom, &style, Block::Section);
            }
            _ if t.starts_with("<#") => {
                let id = t[2..t.len() - 1].split('|').next().unwrap();
                let name = label(
                    &obs.channels,
                    id,
                    '#',
                    SeedUnavailable::MissingChannel(id.into()),
                )?;
                let atom = if pre {
                    Insert::Text(name.into())
                } else {
                    Insert::Atom(json!({"type":"channel","channel_id":id}), name.into())
                };
                push(&mut ops, atom, &style, Block::Section);
            }
            _ if t.starts_with("<!") => {
                let parts = t[2..t.len() - 1]
                    .split('|')
                    .next()
                    .unwrap()
                    .split('^')
                    .collect::<Vec<_>>();
                let (atom, name) = match parts.as_slice() {
                    [range @ ("here" | "channel" | "everyone")] => (
                        json!({"type":"broadcast","range":range}),
                        label(
                            &obs.broadcasts,
                            range,
                            '@',
                            SeedUnavailable::MissingBroadcast((*range).into()),
                        )?
                        .into(),
                    ),
                    ["subteam", id] => (
                        json!({"type":"usergroup","usergroup_id":id}),
                        label(
                            &obs.usergroups,
                            id,
                            '@',
                            SeedUnavailable::MissingUsergroup((*id).into()),
                        )?
                        .into(),
                    ),
                    _ => return Err(SeedUnavailable::UnsupportedToken(t.clone())),
                };
                push(
                    &mut ops,
                    if pre {
                        Insert::Text(name)
                    } else {
                        Insert::Atom(atom, name)
                    },
                    &style,
                    Block::Section,
                );
            }
            _ if t.starts_with('<') => return Err(SeedUnavailable::UnsupportedToken(t.clone())),
            _ => push(&mut ops, Insert::Text(unescape(t)), &style, Block::Section),
        }
    }
    if !matches!(ops.last(),Some(Op{insert:Insert::Text(s),..}) if s.ends_with('\n')) {
        br(&mut ops, block);
    }
    Ok(ops)
}
