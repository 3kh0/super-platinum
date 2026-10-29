//! Private Files module reads. Hits grant display data, never file/action authority.
//! Native routing/metadata and defensive limits are not Gantry wire equivalence.
use super::*;
use serde_json::Value;
use uuid::Uuid;

pub const MAX_REPLY_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PAGE: u32 = 10_000;
pub const MAX_PAGE_ROWS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Relevant,
    Newest,
    Oldest,
    Name,
    NameDescending,
}
impl Sort {
    pub const ALL: [Self; 5] = [
        Self::Relevant,
        Self::Oldest,
        Self::Newest,
        Self::Name,
        Self::NameDescending,
    ];
    pub fn raw(self) -> &'static str {
        match self {
            Self::Relevant => "score",
            Self::Newest => "created",
            Self::Oldest => "timestamp_asc",
            Self::Name => "name",
            Self::NameDescending => "name_desc",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Relevant => "Most relevant",
            Self::Newest => "Newest",
            Self::Oldest => "Oldest",
            Self::Name => "A to Z",
            Self::NameDescending => "Z to A",
        }
    }
    fn wire(self) -> (&'static str, &'static str) {
        match self {
            Self::Relevant => ("score", "desc"),
            Self::Newest => ("created", "desc"),
            Self::Oldest => ("timestamp", "asc"),
            Self::Name => ("name", "asc"),
            Self::NameDescending => ("name", "desc"),
        }
    }
}
/// Values are data, not current preference/account observations. The caller owns
/// a genuinely observed count20/local-sort branch throughout each request.
pub struct Input<'a> {
    pub query: &'a str,
    pub page: u32,
    pub query_session: Uuid,
    pub search_session: Uuid,
    pub sort: Sort,
    pub exclude_bots: Option<bool>,
    pub only_my_channels: Option<bool>,
    pub primary_channel: &'a str,
}
#[derive(Clone)]
pub struct Request {
    pub(in crate::slack) inner: PreparedRequest,
    pub(in crate::slack) destination: String,
    pub(in crate::slack) page: u32,
}
impl Request {
    pub fn requested_page(&self) -> u32 {
        self.page
    }
}
fn invalid() -> Error {
    Error::Api("file_page_unavailable".into())
}
fn identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 255 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}
pub fn request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    input: Input<'_>,
) -> Result<Request, Error> {
    if input.query.trim().is_empty()
        || input.query.len() > 4096
        || input.query.chars().any(char::is_control)
        || !(1..=MAX_PAGE).contains(&input.page)
        || !identifier(input.primary_channel)
        || !identifier(&workspace.team_id)
        || !identifier(&workspace.user_id)
        || workspace.token.is_empty()
    {
        return Err(invalid());
    }
    let (sort, direction) = input.sort.wire();
    // Concrete native cold promoted request, explicitly not restored web history.
    let context=serde_json::json!({"active_cid":input.primary_channel,"recent_filter_in":[],"recent_filter_from":[]}).to_string();
    let mut fields = vec![
        ("module", "files".into()),
        ("query", input.query.into()),
        ("page", input.page.to_string()),
        ("client_req_id", input.query_session.to_string()),
        ("search_session_id", input.search_session.to_string()),
        ("extracts", "1".into()),
        ("highlight", "1".into()),
        ("max_extract_len", "200".into()),
        ("extra_message_data", "1".into()),
        ("no_user_profile", "1".into()),
        ("count", "20".into()),
        ("file_title_only", "false".into()),
        ("query_rewrite_disabled", "false".into()),
        ("include_files_shares", "1".into()),
        ("search_context", "desktop_files_tab".into()),
        ("search_only_team", "".into()),
        ("facets_result_count", "5".into()),
        ("sort", sort.into()),
        ("sort_dir", direction.into()),
        ("max_filter_suggestions", "10".into()),
        ("request_context", context),
        ("search_tab_filter", "files".into()),
        ("search_tab_sort", input.sort.raw().into()),
    ];
    if let Some(value) = input.exclude_bots {
        fields.push(("search_exclude_bots", value.to_string()));
    }
    if let Some(value) = input.only_my_channels {
        fields.push(("search_only_my_channels", value.to_string()));
    }
    let inner = client.file_page_multipart(workspace, fields);
    let url = url::Url::parse(&inner.url).map_err(|_| invalid())?;
    let host = url.host_str().ok_or_else(invalid)?;
    if url.scheme() != "https"
        || !host.ends_with(".slack.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|p| p != 443)
        || url.path() != "/api/search.modules.files"
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(Request {
        destination: host.into(),
        inner,
        page: input.page,
    })
}

/// Detached, escaped display fields only. There is deliberately no File/URL or
/// canonical metadata conversion, even if a search payload contains those keys.
pub struct Hit {
    id: String,
    title: Option<String>,
    name: Option<String>,
    kind: Option<String>,
    user: Option<String>,
    size: Option<u64>,
    tombstoned: bool,
}
impl Hit {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn title(&self) -> &str {
        self.title
            .as_deref()
            .or(self.name.as_deref())
            .unwrap_or("File")
    }
    pub fn kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }
    pub fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }
    pub fn size(&self) -> Option<u64> {
        self.size
    }
    pub fn tombstoned(&self) -> bool {
        self.tombstoned
    }
}
pub struct Page {
    number: u32,
    page_count: u32,
    total: u64,
    hits: Vec<Hit>,
    last: Option<Value>,
}
impl Page {
    pub fn number(&self) -> u32 {
        self.number
    }
    pub fn page_count(&self) -> u32 {
        self.page_count
    }
    pub fn total(&self) -> u64 {
        self.total
    }
    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }
    pub fn last(&self) -> Option<&Value> {
        self.last.as_ref()
    }
    pub fn decode(value: Value, requested: u32) -> Result<Self, Error> {
        if value.get("ok") != Some(&Value::Bool(true)) || !(1..=MAX_PAGE).contains(&requested) {
            return Err(invalid());
        }
        let pagination = value
            .get("pagination")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let number = pagination
            .get("page")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(invalid)?;
        let page_count = pagination
            .get("page_count")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(invalid)?;
        let total = pagination
            .get("total_count")
            .and_then(Value::as_u64)
            .ok_or_else(invalid)?;
        let items = value
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        if number != requested
            || page_count > MAX_PAGE
            || items.len() > MAX_PAGE_ROWS
            || (requested == 1 && page_count == 0 && (total != 0 || !items.is_empty()))
            || (requested == 1 && page_count != 0 && number > page_count)
        {
            return Err(invalid());
        }
        let mut hits = Vec::with_capacity(items.len());
        for item in items {
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| identifier(s))
                .ok_or_else(invalid)?;
            let title =
                text(item.get("search_title"), 16 * 1024)?.or(text(item.get("title"), 16 * 1024)?);
            let name =
                text(item.get("search_name"), 16 * 1024)?.or(text(item.get("name"), 16 * 1024)?);
            let kind = text(item.get("pretty_type"), 1024)?.or(text(item.get("filetype"), 1024)?);
            let user = text(item.get("user"), 255)?;
            let size = match item.get("size") {
                None | Some(Value::Null) => None,
                Some(v) => Some(v.as_u64().ok_or_else(invalid)?),
            };
            hits.push(Hit {
                id: id.into(),
                title,
                name,
                kind,
                user,
                size,
                // Preserve the existing native search-display tombstone veto;
                // these fields confer no metadata or action authority.
                tombstoned: item.get("mode").and_then(Value::as_str) == Some("tombstone")
                    || item.get("is_tombstoned").and_then(Value::as_bool) == Some(true),
            });
        }
        Ok(Self {
            number,
            page_count,
            total,
            hits,
            last: pagination.get("last").cloned(),
        })
    }
}
fn text(value: Option<&Value>, limit: usize) -> Result<Option<String>, Error> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.len() <= limit => Ok(Some(s.clone())),
        _ => Err(invalid()),
    }
}
#[cfg(test)]
mod tests;
