//! Folder-tab membership is distinct from the bookmark folder it references.
use super::{Fault, Inventory, id};
use crate::bookmarks::{Bookmark, valid_link};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    tab: String,
    bookmark: Option<String>,
}
impl Folder {
    pub fn capture(base: &Inventory, tab: &str) -> Result<Self, Fault> {
        let row = base.row(tab).ok_or(Fault::Missing)?;
        if row["type"] != "folder" || id(row) != Some(tab) {
            return Err(Fault::Unsupported);
        }
        let bookmark = match row.pointer("/data/folder_bookmark_id") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s))
                if s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric()) =>
            {
                Some(s.clone())
            }
            _ => return Err(Fault::Shape),
        };
        Ok(Self {
            tab: tab.into(),
            bookmark,
        })
    }
    pub fn tab(&self) -> &str {
        &self.tab
    }
    pub fn bookmark(&self) -> Option<&str> {
        self.bookmark.as_deref()
    }
    pub fn current(&self, base: &Inventory) -> bool {
        Self::capture(base, &self.tab).as_ref() == Ok(self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Open {
    Link(String),
    File(String),
    Unavailable,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub bookmark: Bookmark,
    pub raw: Value,
    pub open: Open,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contents {
    folder: Folder,
    // Full accepted inventory is retained for later exact mutation review.
    raw: Vec<Value>,
    items: Vec<Item>,
}
impl Contents {
    pub fn parse(base: &Inventory, folder: &Folder, response: &Value) -> Result<Self, Fault> {
        if !folder.current(base)
            || response["ok"] != true
            || response.get("error").is_some_and(|e| !e.is_null())
        {
            return Err(Fault::Changed);
        }
        let raw = response["bookmarks"].as_array().ok_or(Fault::Shape)?;
        // Local input budget, not a claim about a server pagination contract.
        if raw.len() > 4096
            || serde_json::to_vec(raw).map_err(|_| Fault::Shape)?.len() > 4 * 1024 * 1024
        {
            return Err(Fault::Limit);
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut items = Vec::new();
        for value in raw {
            let bookmark: Bookmark =
                serde_json::from_value(value.clone()).map_err(|_| Fault::Shape)?;
            if bookmark.id.is_empty()
                || !seen.insert(bookmark.id.clone())
                || bookmark
                    .channel_id
                    .as_deref()
                    .is_some_and(|c| c != base.channel())
            {
                return Err(Fault::Shape);
            }
            if bookmark.folder()
                || bookmark.parent_id.as_deref().filter(|s| !s.is_empty()) != folder.bookmark()
            {
                continue;
            }
            let open = match bookmark.kind.as_deref() {
                Some("link") => bookmark
                    .link
                    .as_deref()
                    .filter(|s| valid_link(s))
                    .map(|s| Open::Link(s.into()))
                    .unwrap_or(Open::Unavailable),
                Some("file") => value
                    .get("entity_id")
                    .and_then(Value::as_str)
                    .filter(|s| super::resource_id(s))
                    .map(|s| Open::File(s.into()))
                    .unwrap_or(Open::Unavailable),
                _ => Open::Unavailable,
            };
            items.push(Item {
                bookmark,
                raw: value.clone(),
                open,
            });
        }
        items.sort_by(|a, b| a.bookmark.rank.cmp(&b.bookmark.rank));
        Ok(Self {
            folder: folder.clone(),
            raw: raw.clone(),
            items,
        })
    }
    pub fn folder(&self) -> &Folder {
        &self.folder
    }
    pub fn items(&self) -> &[Item] {
        &self.items
    }
    pub fn raw(&self) -> &[Value] {
        &self.raw
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod operations_tests;
