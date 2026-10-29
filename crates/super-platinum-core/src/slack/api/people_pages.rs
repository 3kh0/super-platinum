//! Numbered People responses, separate from the bounded Edge directory lookup.
//! Limits below are defensive native support policy, not service guarantees.
//! Opaque native XParams/routing are deliberately not browser-metadata parity.
use super::*;
use serde_json::Value;
use uuid::Uuid;
mod account_type;
pub use account_type::AccountType;

pub const MAX_REPLY_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PAGE: u32 = 10_000;
pub const MAX_PAGE_ROWS: usize = 100;
pub const MAX_QUERY_BYTES: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Relevant,
    Name,
    NameDescending,
}
impl Sort {
    pub fn preference_value(self) -> &'static str {
        match self {
            Self::Relevant => "relevant",
            Self::Name => "name",
            Self::NameDescending => "name_desc",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "relevant" => Some(Self::Relevant),
            "name" => Some(Self::Name),
            "name_desc" => Some(Self::NameDescending),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Relevant => "Most relevant",
            Self::Name => "Name A–Z",
            Self::NameDescending => "Name Z–A",
        }
    }
    fn wire(self) -> (&'static str, &'static str) {
        match self {
            Self::Relevant => ("relevant", "desc"),
            Self::Name => ("name", "asc"),
            Self::NameDescending => ("name", "desc"),
        }
    }
}

/// Caller must own genuinely current supported controls, accepted count20/sort/hide
/// observations, and original account/query/page authority throughout the read.
/// Construction validates data; it never supplies those observations or authority.
pub struct Input<'a> {
    pub query: &'a str,
    pub page: u32,
    pub query_session: Uuid,
    pub search_session: Uuid,
    pub sort: Sort,
    pub hide_deactivated: bool,
    /// Explicit native Home-primary mapping, not a source sibling-view observation.
    pub primary_channel: &'a str,
}

// No Debug, public raw request conversion, arbitrary endpoint or file parts.
#[derive(Clone)]
pub struct Request {
    pub(in crate::slack) inner: PreparedRequest,
    pub(in crate::slack) destination: String,
    pub(in crate::slack) page: u32,
}
fn invalid() -> Error {
    Error::Api("people_page_unavailable".into())
}
fn identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 255 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}
pub fn request(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    input: Input<'_>,
) -> Result<Request, Error> {
    request_for_account_type(client, workspace, input, AccountType::Any)
}
/// Explicit owned filter data, never an observation or account authority. The
/// original entry point retains its identical default field shape.
pub fn request_for_account_type(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    input: Input<'_>,
    account_type: AccountType,
) -> Result<Request, Error> {
    if input.query.trim().is_empty()
        || input.query.len() > MAX_QUERY_BYTES
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
    // Deliberately owned cold controls and promoted empty filter history. These
    // are not inferred from absent/restored web state or a workspace User DTO.
    let context = format!(
        "{{\"active_cid\":{},\"recent_filter_in\":[],\"recent_filter_from\":[]}}",
        serde_json::to_string(input.primary_channel).map_err(|_| invalid())?
    );
    let mut fields = vec![
        ("module", "people".into()),
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
        ("search_context", "desktop_people_tab".into()),
        ("tz", "".into()),
        ("external_shared_team_ids", "".into()),
        ("custom_fields", "{}".into()),
        (
            "hide_deactivated_users",
            if input.hide_deactivated { "1" } else { "0" }.into(),
        ),
        ("request_context", context),
        ("spell_correction", "FUZZY_MATCH".into()),
        ("sort", sort.into()),
        ("sort_dir", direction.into()),
        ("search_only_team", "".into()),
        ("profile_location", "".into()),
        ("profile_title", "".into()),
        ("profile_division", "".into()),
        ("profile_department", "".into()),
        ("profile_celebration", "".into()),
    ];
    if let Some(value) = account_type.wire() {
        fields.push(("account_type", value.into()));
    }
    let inner = client.people_page_multipart(workspace, fields);
    let url = url::Url::parse(&inner.url).map_err(|_| invalid())?;
    let host = url.host_str().ok_or_else(invalid)?;
    if url.scheme() != "https"
        || !host.ends_with(".slack.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
        || url.path() != "/api/search.modules.people"
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

/// Search hit metadata is partial. It is never a canonical User/profile, even
/// when its id is already known. Keep duplicate rows and their original order.
pub struct Hit {
    id: String,
    highlight: Option<String>,
    iid: Option<Value>,
}
impl Hit {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn highlight(&self) -> Option<&str> {
        self.highlight.as_deref()
    }
    pub fn iid(&self) -> Option<&Value> {
        self.iid.as_ref()
    }
}
pub struct Page {
    number: u32,
    page_count: u32,
    total: u64,
    request_id: Option<String>,
    hits: Vec<Hit>,
    raw: Arc<Value>,
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
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }
    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }
    pub fn raw(&self) -> &Value {
        &self.raw
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
            || (page_count == 0 && (requested != 1 || total != 0 || !items.is_empty()))
            || (page_count != 0 && number > page_count)
        {
            return Err(invalid());
        }
        let request_id = text(value.get("request_id"), 4096)?;
        let mut hits = Vec::with_capacity(items.len());
        for item in items {
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .filter(|v| identifier(v))
                .ok_or_else(invalid)?;
            let profile = item.get("profile").filter(|v| !v.is_null());
            if profile.is_some_and(|p| !p.is_object()) {
                return Err(invalid());
            }
            let highlight = text(profile.and_then(|p| p.get("display_name")), 16 * 1024)?;
            let iid = item.get("iid").filter(|v| !v.is_null()).cloned();
            if iid
                .as_ref()
                .is_some_and(|v| !v.is_number() && !v.as_str().is_some_and(|s| s.len() <= 4096))
            {
                return Err(invalid());
            }
            hits.push(Hit {
                id: id.into(),
                highlight,
                iid,
            });
        }
        Ok(Self {
            number,
            page_count,
            total,
            request_id,
            hits,
            raw: Arc::new(value),
        })
    }
}
fn text(value: Option<&Value>, max: usize) -> Result<Option<String>, Error> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.len() <= max => Ok(Some(s.clone())),
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod tests;
