//! The retained B 3bvJ length reducer, after E1L/ has resolved inline tokens.
//! Strings use UTF-16 units, and Delta merges adjacent identical attributes.
//! Resolution of stateful labels/catalog aliases belongs to the caller; raw
//! rich JSON does not contain enough information to infer those values.

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Kind {
    Text {
        link: Option<String>,
    },
    Mention {
        id: String,
        label: String,
    },
    TabMention {
        id: String,
        label: String,
        tab: String,
    },
    Slug {
        identity: String,
    },
    Emoji,
}

#[derive(Clone)]
struct Op {
    text: String,
    kind: Kind,
    style: [bool; 6],
}

#[derive(Default)]
pub(super) struct Delta {
    ops: Vec<Op>,
}

impl Delta {
    pub fn insert(&mut self, text: String, kind: Kind, style: [bool; 6]) {
        // Embeds remain individual operations even with identical attributes.
        if text.is_empty() && kind != Kind::Emoji {
            return;
        }
        if kind != Kind::Emoji
            && let Some(last) = self.ops.last_mut()
            && last.kind == kind
            && last.style == style
        {
            last.text.push_str(&text);
            return;
        }
        self.ops.push(Op { text, kind, style });
    }

    pub fn newline(&mut self) {
        self.insert("\n".into(), Kind::Text { link: None }, [false; 6]);
    }

    pub fn trailing(&self) -> bool {
        self.ops
            .last()
            .is_some_and(|op| op.kind != Kind::Emoji && op.text.ends_with('\n'))
    }

    pub fn length(&self) -> usize {
        let mut total = 0;
        for op in &self.ops {
            let length = op.text.encode_utf16().count();
            match &op.kind {
                Kind::Text { link: Some(url) } if url != &op.text => {
                    total += length + url.encode_utf16().count();
                }
                Kind::Mention { id, .. } if builtin(id) || id == "UNVERIFIED" => {
                    // Captured 3bvJ returns insert.length here, not total +
                    // insert.length. Preserve this observed reducer behavior.
                    total = length;
                }
                Kind::Mention { id, .. } | Kind::TabMention { id, .. } if !id.is_empty() => {
                    total += id.encode_utf16().count();
                }
                _ => total += length,
            }
        }
        if self.trailing() {
            total.saturating_sub(1)
        } else {
            total
        }
    }
}

pub(super) fn builtin(id: &str) -> bool {
    // S nPsK = Object.keys(kSm/.N) plus BKgroup.
    matches!(
        id,
        "BKeveryone"
            | "BKchannel"
            | "BKhere"
            | "BKsf:accountteam"
            | "BKsf:dealteam"
            | "BKassistant:einstein"
            | "BKgroup"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_source_oracle_operations_match() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("delta_oracle.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let mut delta = Delta::default();
            for op in case["ops"].as_array().unwrap() {
                let text = op["text"].as_str().unwrap().to_owned();
                let kind = match op["kind"].as_str().unwrap() {
                    "text" => Kind::Text {
                        link: op["link"].as_str().map(str::to_owned),
                    },
                    "mention" => Kind::Mention {
                        id: op["id"].as_str().unwrap().into(),
                        label: op["label"].as_str().unwrap().into(),
                    },
                    "slug" => Kind::Slug {
                        identity: op["identity"].as_str().unwrap().into(),
                    },
                    "emoji" => Kind::Emoji,
                    _ => panic!("unexpected oracle operation"),
                };
                let mut style = [false; 6];
                style[0] = op["bold"] == true;
                delta.insert(text, kind, style);
            }
            assert_eq!(
                delta.length(),
                case["expected"].as_u64().unwrap() as usize,
                "{}",
                case["name"]
            );
        }
    }
}
