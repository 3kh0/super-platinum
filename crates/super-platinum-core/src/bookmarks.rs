//! Channel bookmarks, with scoped reads and guarded link editing.
use serde::Deserialize;
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Bookmark {
    pub id: String,
    #[serde(default, alias = "channel")]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default, rename = "type", alias = "bookmark_type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub rank: Option<String>,
    #[serde(default, alias = "shortcutId")]
    pub shortcut_id: Option<String>,
}
impl Bookmark {
    pub fn title(&self) -> &str {
        self.title
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(self.link.as_deref())
            .unwrap_or("Untitled bookmark")
    }
    pub fn editable(&self) -> bool {
        self.kind.as_deref() == Some("link") || self.folder()
    }
    pub fn folder(&self) -> bool {
        matches!(self.kind.as_deref(), Some("local_folder" | "folder"))
    }
    pub fn same_content(&self, other: &Self) -> bool {
        self.id == other.id
            && self.kind == other.kind
            && self.title == other.title
            && self.link == other.link
            && self.emoji == other.emoji
            && self.parent_id == other.parent_id
            && self.shortcut_id == other.shortcut_id
    }
}
pub fn valid_link(value: &str) -> bool {
    url::Url::parse(value.trim()).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https")
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
    })
}
#[derive(Clone, Debug)]
pub struct Draft {
    pub base: Option<Bookmark>,
    pub remove: bool,
    pub folder: bool,
    pub movement: Option<Movement>,
    pub contents: Vec<Bookmark>,
    pub title: String,
    pub link: String,
    pub error: Option<String>,
    pub uncertain: bool,
}
impl Draft {
    pub fn new(base: Option<Bookmark>, remove: bool) -> Self {
        Self {
            title: base
                .as_ref()
                .map(|b| b.title().to_owned())
                .unwrap_or_default(),
            link: base
                .as_ref()
                .and_then(|b| b.link.clone())
                .unwrap_or_default(),
            folder: base.as_ref().is_some_and(Bookmark::folder),
            movement: None,
            contents: Vec::new(),
            base,
            remove,
            error: None,
            uncertain: false,
        }
    }
    pub fn valid(&self) -> bool {
        self.remove
            || self.movement.is_some()
            || (!self.title.trim().is_empty() && (self.folder || valid_link(&self.link)))
    }
}
#[derive(Debug, Default)]
pub struct BookmarksState {
    pub scope: Option<(u64, String, String)>,
    pub generation: u64,
    pub items: Vec<Bookmark>,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
    pub busy: bool,
    pub draft: Option<Draft>,
    pub collapsed: std::collections::HashSet<String>,
}
impl BookmarksState {
    pub fn matches_scope(&self, epoch: u64, team: &str, channel: &str) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|(e, t, c)| *e == epoch && t == team && c == channel)
    }
    pub fn ensure_scope(&mut self, epoch: u64, team: &str, channel: &str) {
        if !self.matches_scope(epoch, team, channel) {
            *self = Self {
                scope: Some((epoch, team.into(), channel.into())),
                generation: self.generation.wrapping_add(1),
                ..Default::default()
            };
        }
    }
    pub fn begin(&mut self) -> Option<u64> {
        if self.busy {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        self.error = None;
        Some(self.generation)
    }
    pub fn apply(&mut self, generation: u64, mut items: Vec<Bookmark>) -> bool {
        if generation != self.generation || self.busy {
            return false;
        }
        let mut seen = std::collections::HashSet::new();
        items.retain(|item| {
            !item.id.is_empty()
                && seen.insert(item.id.clone())
                && item.channel_id.as_deref().is_none_or(|c| {
                    self.scope
                        .as_ref()
                        .is_some_and(|(_, _, channel)| channel == c)
                })
        });
        items.sort_by(|a, b| a.rank.cmp(&b.rank));
        self.items = items;
        self.loading = false;
        self.loaded = true;
        self.error = None;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_reads_cannot_replace_newer_scopes_or_pending_edits() {
        let mut s = BookmarksState::default();
        s.ensure_scope(1, "T", "C");
        let old = s.begin().unwrap();
        let latest = s.begin().unwrap();
        assert!(!s.apply(old, vec![]));
        assert!(s.apply(latest, vec![]));
        let old = s.begin().unwrap();
        s.ensure_scope(1, "T", "D");
        assert!(!s.apply(old, vec![]));
        let old = s.begin().unwrap();
        s.ensure_scope(2, "T", "D");
        assert!(!s.apply(old, vec![]));
        s.busy = true;
        assert!(s.begin().is_none());
        assert!(!s.apply(s.generation, vec![]));
    }
    #[test]
    fn link_validation_and_preflight_preserve_scope_and_metadata() {
        assert!(valid_link(" https://example.com/path?q=日本語 "));
        for bad in [
            "javascript:alert(1)",
            "file:///tmp/file",
            "https://user:pass@example.com",
            "data:text/html,hello",
            "",
        ] {
            assert!(!valid_link(bad));
        }
        let a:Bookmark=serde_json::from_value(serde_json::json!({"id":"Bk1","title":"Docs","link":"https://example.com","type":"link","emoji":null,"parent_id":"BkFolder","rank":"a"})).unwrap();
        let mut b = a.clone();
        b.rank = Some("b".into());
        assert!(a.same_content(&b));
        b.link = Some("https://example.org".into());
        assert!(!a.same_content(&b));
        assert!(Draft::new(Some(a), false).valid());
    }
    #[test]
    fn lists_preserve_folders_and_sort_without_foreign_channel_rows() {
        let mut s = BookmarksState::default();
        s.ensure_scope(1, "T", "C");
        let generation = s.begin().unwrap();
        let items = serde_json::from_value(serde_json::json!([
            {"id":"B2","channel_id":"C","type":"link","rank":"b","parent_id":"B1"},
            {"id":"B1","channel_id":"C","type":"local_folder","rank":"a"},
            {"id":"B1","channel_id":"C"},{"id":"foreign","channel_id":"D"}
        ]))
        .unwrap();
        assert!(s.apply(generation, items));
        assert_eq!(s.items.len(), 2);
        assert_eq!(s.items[0].id, "B1");
        assert!(s.items[0].folder());
        assert!(s.items[0].editable());
    }
}

#[derive(Clone, Debug, Default)]
pub struct Movement {
    pub parent: String,
    pub before: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub parent: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
}

pub fn descendants<'a>(items: &'a [Bookmark], id: &str) -> Vec<&'a Bookmark> {
    let mut seen = std::collections::HashSet::from([id.to_owned()]);
    let mut output = Vec::new();
    loop {
        let mut added = false;
        for item in items {
            if !seen.contains(&item.id) && item.parent_id.as_ref().is_some_and(|p| seen.contains(p))
            {
                seen.insert(item.id.clone());
                output.push(item);
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    output.sort_by(|a, b| a.id.cmp(&b.id));
    output
}

/// Recheck the exact deletion scope shown in the confirmation.
pub fn removal_matches(items: &[Bookmark], base: &Bookmark, expected: &[Bookmark]) -> bool {
    if !items.iter().any(|item| item.same_content(base)) {
        return false;
    }
    if !base.folder() {
        return true;
    }
    let actual = descendants(items, &base.id);
    actual.len() == expected.len()
        && actual
            .iter()
            .all(|item| expected.iter().any(|old| item.same_content(old)))
}

/// Compute placement from the latest list and explicit destination, never from
/// stale numeric positions. Only links can be put inside folders.
pub fn placement(
    items: &[Bookmark],
    base: &Bookmark,
    movement: &Movement,
) -> Result<Placement, &'static str> {
    let current = items
        .iter()
        .find(|b| b.id == base.id)
        .ok_or("bookmark_changed")?;
    if !current.same_content(base) || current.rank != base.rank {
        return Err("bookmark_changed");
    }
    let parent = (!movement.parent.is_empty()).then_some(movement.parent.as_str());
    if let Some(parent) = parent {
        if base.folder()
            || parent == base.id
            || descendants(items, &base.id).iter().any(|b| b.id == parent)
            || !items.iter().any(|b| b.id == parent && b.folder())
        {
            return Err("invalid_destination");
        }
    }
    let mut siblings = items
        .iter()
        .filter(|b| b.id != base.id && b.parent_id.as_deref().filter(|p| !p.is_empty()) == parent)
        .collect::<Vec<_>>();
    siblings.sort_by(|a, b| a.rank.cmp(&b.rank));
    let position = if movement.before.is_empty() {
        siblings.len()
    } else {
        siblings
            .iter()
            .position(|b| b.id == movement.before)
            .ok_or("bookmark_changed")?
    };
    Ok(Placement {
        parent: parent.map(str::to_owned),
        left: position.checked_sub(1).map(|i| siblings[i].id.clone()),
        right: siblings.get(position).map(|b| b.id.clone()),
    })
}

pub fn rows<'a>(
    items: &'a [Bookmark],
    collapsed: &std::collections::HashSet<String>,
) -> Vec<(&'a Bookmark, usize)> {
    fn visit<'a>(
        item: &'a Bookmark,
        depth: usize,
        items: &[&'a Bookmark],
        original: &[Bookmark],
        collapsed: &std::collections::HashSet<String>,
        seen: &mut std::collections::HashSet<String>,
        output: &mut Vec<(&'a Bookmark, usize)>,
    ) {
        if !seen.insert(item.id.clone()) {
            return;
        }
        output.push((item, depth));
        if collapsed.contains(&item.id) {
            for child in descendants(original, &item.id) {
                seen.insert(child.id.clone());
            }
            return;
        }
        if item.folder() {
            for child in items
                .iter()
                .filter(|b| b.parent_id.as_deref() == Some(&item.id))
            {
                visit(child, depth + 1, items, original, collapsed, seen, output);
            }
        }
    }
    let mut sorted = items.iter().collect::<Vec<_>>();
    sorted.sort_by(|a, b| a.rank.cmp(&b.rank));
    let mut seen = std::collections::HashSet::new();
    let mut output = Vec::new();
    for item in &sorted {
        if !item
            .parent_id
            .as_deref()
            .is_some_and(|p| p != item.id && items.iter().any(|b| b.id == p && b.folder()))
        {
            visit(item, 0, &sorted, items, collapsed, &mut seen, &mut output);
        }
    }
    // Preserve malformed/orphaned/cyclic server entries without recursing forever.
    for item in &sorted {
        visit(item, 0, &sorted, items, collapsed, &mut seen, &mut output);
    }
    output
}

#[cfg(test)]
mod organization_tests {
    use super::*;
    fn items() -> Vec<Bookmark> {
        serde_json::from_value(serde_json::json!([
            {"id":"F","title":"Folder","type":"local_folder","rank":"m"},
            {"id":"A","title":"A","type":"link","parent_id":"F","rank":"b"},
            {"id":"B","title":"B","type":"link","rank":"z"}
        ]))
        .unwrap()
    }
    #[test]
    fn hierarchy_preserves_all_rows_and_collapses_only_descendants() {
        let items = items();
        let rows = rows(&items, &Default::default());
        assert_eq!(
            rows.iter()
                .map(|(b, d)| (b.id.as_str(), *d))
                .collect::<Vec<_>>(),
            vec![("F", 0), ("A", 1), ("B", 0)]
        );
        assert_eq!(
            super::rows(&items, &std::collections::HashSet::from(["F".into()])).len(),
            2
        );
        let mut cyclic = items;
        cyclic[0].parent_id = Some("F".into());
        assert_eq!(super::rows(&cyclic, &Default::default()).len(), 3);
    }
    #[test]
    fn folder_removal_rejects_changed_confirmation_contents() {
        let original = items();
        let base = &original[0];
        let expected = vec![original[1].clone()];
        assert!(removal_matches(&original, base, &expected));
        for case in 0..5 {
            let mut current = original.clone();
            match case {
                0 => current[2].parent_id = Some(base.id.clone()),
                1 => current[1].parent_id = None,
                2 => current[1].title = Some("Renamed elsewhere".into()),
                3 => current[0].title = Some("Folder renamed".into()),
                _ => {
                    current.remove(1);
                }
            }
            assert!(!removal_matches(&current, base, &expected), "case {case}");
        }
        let mut reordered = original.clone();
        reordered.reverse();
        assert!(removal_matches(&reordered, base, &expected));
    }
    #[test]
    fn movement_uses_current_anchors_and_rejects_stale_or_invalid_destinations() {
        let items = items();
        let base = &items[1];
        assert_eq!(
            placement(
                &items,
                base,
                &Movement {
                    parent: "".into(),
                    before: "B".into()
                }
            )
            .unwrap(),
            Placement {
                parent: None,
                left: Some("F".into()),
                right: Some("B".into())
            }
        );
        assert_eq!(
            placement(
                &items,
                &items[2],
                &Movement {
                    parent: "F".into(),
                    before: "A".into()
                }
            )
            .unwrap()
            .left,
            None
        );
        assert!(
            placement(
                &items,
                &items[0],
                &Movement {
                    parent: "F".into(),
                    before: "".into()
                }
            )
            .is_err()
        );
        let mut changed = items.clone();
        changed[1].rank = Some("y".into());
        assert!(placement(&changed, base, &Movement::default()).is_err());
        assert!(
            placement(
                &items,
                base,
                &Movement {
                    parent: "gone".into(),
                    before: "".into()
                }
            )
            .is_err()
        );
        assert!(
            placement(
                &items,
                base,
                &Movement {
                    parent: "".into(),
                    before: "gone".into()
                }
            )
            .is_err()
        );
    }
}
