//! Ordinary native representation of the current Channels search-module recipe.
//! Pages are passive metadata, never canonical membership/history/action proof.
//! Callers must separately own the account, query and public-vs-joined policy.
use super::*;

const ENDPOINT: &str = "search.modules.channels";
const COUNT: usize = 50;
const MAX_ROWS: usize = 200;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Joined,
    Accessible,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Name,
    NameReverse,
    MostMembers,
    FewestMembers,
    CreatedNewest,
    CreatedOldest,
    Relevant,
}
/// Explicit type choices from MNeC.xV; 3yhc maps Shared to external_shared.
/// Active retains this native browser's deliberately restricted active subset.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Active,
    Public,
    Private,
    Archived,
    Shared,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub scope: Scope,
    pub kind: Kind,
    pub sort: Sort,
    pub page: u32,
    pub cursor: Option<String>,
}
impl Query {
    fn valid(&self) -> bool {
        self.page > 0
            && self.page <= 10_000
            && self.text.len() <= 4096
            && !self.text.contains('\0')
            && self.cursor.as_ref().is_none_or(|s| {
                !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
            })
    }
}

/// Explicit native active-channel/name-sort subset, not an SDK default claim.
/// Source current-channel-directory-{fetch,method}-static-20261030-01 pins
/// the endpoint, base count, sort mapping and browse/text flags. Effective SDK
/// providers, normalization and role admission remain separate boundaries.
pub fn request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    query: &Query,
) -> Result<PreparedRequest, Error> {
    if !query.valid() {
        return Err(Error::Api("invalid_channel_directory_query".into()));
    }
    let (sort, direction) = match query.sort {
        Sort::Name => ("name", "asc"),
        Sort::NameReverse => ("name", "desc"),
        Sort::MostMembers => ("member_count", "desc"),
        Sort::FewestMembers => ("member_count", "asc"),
        Sort::CreatedNewest => ("created", "desc"),
        Sort::CreatedOldest => ("created", "asc"),
        Sort::Relevant => ("relevant", "desc"),
    };
    let text = query.text.trim();
    let mut fields = vec![
        ("module", "channels".into()),
        ("query", text.into()),
        ("page", query.page.to_string()),
        ("count", COUNT.to_string()),
        ("sort", sort.into()),
        ("sort_dir", direction.into()),
        (
            "channel_type",
            match query.kind {
                Kind::Active => "exclude_archived",
                Kind::Public => "private_exclude",
                Kind::Private => "private",
                Kind::Archived => "archived",
                Kind::Shared => "external_shared",
            }
            .into(),
        ),
        ("exclude_my_channels", "0".into()),
        (
            "search_only_my_channels",
            (query.scope == Scope::Joined).to_string(),
        ),
        ("max_filter_suggestions", "10".into()),
        ("extra_message_data", "0".into()),
        ("no_user_profile", "1".into()),
        ("include_files_shares", "1".into()),
        ("file_title_only", "false".into()),
        ("query_rewrite_disabled", "false".into()),
        ("highlight", if text.is_empty() { "0" } else { "1" }.into()),
        ("extracts", if text.is_empty() { "0" } else { "1" }.into()),
    ];
    if text.is_empty() {
        fields.push(("browse", "standard".into()));
    } else {
        fields.push(("max_extract_len", "200".into()));
    }
    if let Some(cursor) = &query.cursor {
        fields.push(("cursor", cursor.clone()));
    }
    Ok(client.rest_form(workspace, ENDPOINT, fields))
}

#[derive(Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub name: Option<String>,
    pub purpose: Option<String>,
    pub member_count: Option<u64>,
    pub is_member: Option<bool>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Page {
    pub items: Vec<Item>,
    pub page: u32,
    pub next_cursor: Option<String>,
    pub total: u64,
    pub pages: u32,
}

fn malformed() -> Error {
    Error::Transport("decode search.modules.channels: invalid metadata page".into())
}

/// Validate the page against its request; neither a result ID nor is_member
/// grants navigation, joining, canonical publication, file or history access.
pub fn decode(value: serde_json::Value, query: &Query) -> Result<Page, Error> {
    if !query.valid() {
        return Err(malformed());
    }
    if value.get("ok") != Some(&serde_json::Value::Bool(true)) {
        return Err(Error::Api("channel_directory_search_failed".into()));
    }
    if value.get("error").is_some_and(|v| !v.is_null()) {
        return Err(malformed());
    }
    let rows = value
        .get("items")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(malformed)?;
    if rows.len() > MAX_ROWS {
        return Err(malformed());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row
            .get("id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| {
                !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric())
            })
            .ok_or_else(malformed)?;
        if !seen.insert(id) {
            return Err(malformed());
        }
        let is_member = match row.get("is_member") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::Bool(v)) => Some(*v),
            _ => return Err(malformed()),
        };
        if query.scope == Scope::Joined && is_member != Some(true) {
            return Err(malformed());
        }
        let member_count = match row.get("member_count") {
            None | Some(serde_json::Value::Null) => None,
            Some(v) => Some(v.as_u64().ok_or_else(malformed)?),
        };
        items.push(Item {
            id: id.into(),
            name: row
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            purpose: row
                .get("purpose")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            member_count,
            is_member,
        });
    }
    let pagination = value.get("pagination");
    if pagination.is_some_and(|v| !v.is_object() && !v.is_null()) {
        return Err(malformed());
    }
    let number = |name: &str, fallback: u64| -> Result<u64, Error> {
        match pagination.and_then(|p| p.get(name)) {
            None | Some(serde_json::Value::Null) => Ok(fallback),
            Some(v) => v.as_u64().ok_or_else(malformed),
        }
    };
    let page = u32::try_from(number("page", 1)?.max(1)).map_err(|_| malformed())?;
    if page != query.page {
        return Err(malformed());
    }
    let total = match number("total_count", items.len() as u64)? {
        0 => items.len() as u64,
        count => count,
    };
    let pages = u32::try_from(number("page_count", 1)?.max(1)).map_err(|_| malformed())?;
    let mut next_cursor = match pagination.and_then(|p| p.get("next_cursor")) {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) if s.is_empty() => None,
        Some(serde_json::Value::String(s))
            if s.len() <= 4096 && !s.chars().any(char::is_control) =>
        {
            Some(s.clone())
        }
        _ => return Err(malformed()),
    };
    // The current response formatter clears its cursor on an empty items page.
    if items.is_empty() {
        next_cursor = None;
    } else if next_cursor.is_some() && next_cursor == query.cursor {
        return Err(malformed());
    }
    Ok(Page {
        items,
        page,
        next_cursor,
        total,
        pages,
    })
}

#[cfg(test)]
mod tests;
