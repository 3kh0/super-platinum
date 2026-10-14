//! Boolean equivalent of captured `7YoG(undefined)` used by `ehbb`.
//! The default `zllF` data is selected (no workspace emoji_v17 experiment).
//! A UTF-16 trie avoids compiling a thousands-alternative backtracking regexp.
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Default)]
struct Node {
    edges: BTreeMap<u16, usize>,
    terminal: bool,
}
fn trie() -> &'static [Node] {
    static TRIE: OnceLock<Vec<Node>> = OnceLock::new();
    TRIE.get_or_init(|| {
        let glyphs: Vec<String> = serde_json::from_str(include_str!("emoji.json"))
            .expect("captured default Slack native emoji data");
        let mut nodes = vec![Node::default()];
        for glyph in glyphs {
            let mut index = 0;
            for unit in glyph.encode_utf16() {
                index = match nodes[index].edges.get(&unit) {
                    Some(next) => *next,
                    None => {
                        let next = nodes.len();
                        nodes.push(Node::default());
                        nodes[index].edges.insert(unit, next);
                        next
                    }
                };
            }
            nodes[index].terminal = true;
        }
        nodes
    })
}
pub(super) fn contains(value: &str) -> bool {
    let units = value.encode_utf16().collect::<Vec<_>>();
    let nodes = trie();
    for start in 0..units.len() {
        let mut node = 0;
        for end in start..units.len() {
            let Some(next) = nodes[node].edges.get(&units[end]) else {
                break;
            };
            node = *next;
            // Captured regexp is non-u: its literal skin-modifier character
            // class contains D83C and DFFB..DFFF code units, not scalar values.
            // Optional trailing skin tone never affects boolean acceptance.
            if nodes[node].terminal
                && !units
                    .get(end + 1)
                    .is_some_and(|u| matches!(*u, 0x200d | 0xd83c | 0xdffb..=0xdfff))
            {
                return true;
            }
        }
    }
    false
}
