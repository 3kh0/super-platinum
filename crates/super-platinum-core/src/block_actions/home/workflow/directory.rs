//! Captured WNgx/53BY/vAgx request shapes. The hXqI channel caller uses only
//! Channels remotely; includeIms/includeMpims are local-store branches.
//! Returned candidates still require current local member/posting policy.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Directory {
    Users,
    Channels,
    GroupMessages,
}

/// Raw page from the captured 2gUC wrapper. Membership is independent of result
/// presence. This decoder never marks a result selectable or opens a DM.
#[derive(Clone)]
pub struct DirectoryPage {
    raw: Value,
    rows: Vec<Value>,
    members: Vec<String>,
    marker: Option<String>,
}
impl DirectoryPage {
    pub fn decode(raw: Value) -> Result<Self, Error> {
        if !raw.is_object() || raw.get("ok").is_some_and(|v| v != true) {
            return Err(invalid());
        }
        let mut rows = Vec::new();
        match raw.get("results") {
            Some(Value::Array(a)) => rows.extend(a.iter().cloned()),
            Some(Value::Object(o)) => {
                for group in o.values() {
                    match group {
                        Value::Array(a) => rows.extend(a.iter().cloned()),
                        Value::Object(_) => rows.push(group.clone()),
                        _ => return Err(invalid()),
                    }
                }
            }
            None => {}
            _ => return Err(invalid()),
        }
        if let Some(mpims) = raw.get("mpims") {
            rows.extend(mpims.as_array().ok_or_else(invalid)?.iter().cloned());
        }
        if rows.len() > 1000
            || rows
                .iter()
                .any(|v| !v.is_object() || v["id"].as_str().is_none_or(|s| !id(s)))
        {
            return Err(invalid());
        }
        let members = match raw.get("member_channels") {
            None => vec![],
            Some(v) => v
                .as_array()
                .filter(|a| a.len() <= 1000)
                .ok_or_else(invalid)?
                .iter()
                .map(|v| {
                    v.as_str()
                        .filter(|s| id(s))
                        .map(str::to_owned)
                        .ok_or_else(invalid)
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        let marker = match raw.get("next_marker") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s)) if s.len() <= 1024 && !s.chars().any(char::is_control) => {
                Some(s.clone())
            }
            _ => return Err(invalid()),
        };
        Ok(Self {
            raw,
            rows,
            members,
            marker,
        })
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub fn member_channels(&self) -> &[String] {
        &self.members
    }
    pub fn next_marker(&self) -> Option<&str> {
        self.marker.as_deref()
    }
}
impl Preflight {
    pub fn directory_page_request(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        current: &View,
        endpoint: &DirectoryEndpoint,
        now: std::time::Instant,
        field: &str,
        custom: bool,
        directory: Directory,
        query: &str,
        marker: Option<&str>,
        only_from_team: Option<&str>,
    ) -> Result<PreparedRequest, Error> {
        let mut request = self.directory_request(
            client, w, current, endpoint, now, field, custom, directory, query,
        )?;
        if let Some(team) = only_from_team {
            // hXqI scopes only its missing-context channel picker; 53BY emits
            // external_team. The caller retains the actual j08h witness.
            if custom || directory != Directory::Channels || !id(team) {
                return Err(invalid());
            }
            let crate::slack::client::RequestBody::Json(raw) = &mut request.body else {
                return Err(invalid());
            };
            raw["external_team"] = json!(team);
        }
        if let Some(marker) = marker {
            if marker.is_empty() || marker.len() > 1024 || marker.chars().any(char::is_control) {
                return Err(invalid());
            }
            let crate::slack::client::RequestBody::Json(raw) = &mut request.body else {
                return Err(invalid());
            };
            raw["marker"] = json!(marker);
        }
        Ok(request)
    }
    pub fn directory_request(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
        current: &View,
        endpoint: &DirectoryEndpoint,
        now: std::time::Instant,
        field: &str,
        custom: bool,
        directory: Directory,
        query: &str,
    ) -> Result<PreparedRequest, Error> {
        if !self.workflow.same_view(current)
            || self.workflow.user != w.user_id
            || query.len() > 1024
            || query.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        let input = (if custom {
            self.custom()
        } else {
            self.context()
        })
        .iter()
        .find(|i| i.name == field)
        .ok_or_else(invalid)?;
        let (path, body) = match (&input.kind, directory) {
            (InputType::User, Directory::Users) => (
                "users/search",
                json!({
                    "query":query,"count":24,"fuzz":1,"enable_workspace_ranking":true,
                    "filter":"NOT deactivated","include_profile_only_users":true
                }),
            ),
            (InputType::Conversation, Directory::Channels) => (
                "channels/search",
                json!({
                    "query":query,"count":100,"fuzz":1,"include_record_channels":true,"check_membership":true
                }),
            ),
            (InputType::Conversation, Directory::GroupMessages) => (
                "mpims/search",
                json!({
                    "query":normalize_mpim(query),"count":100,"fuzz":1,"use_display_name":true
                }),
            ),
            _ => return Err(invalid()),
        };
        endpoint.request(client, w, now, path, body)
    }
}

fn normalize_mpim(query: &str) -> String {
    // vAgx /,\s/g removes precisely one whitespace character after each comma.
    // Do not trim an arbitrary user query or collapse a remaining second space.
    let mut chars = query.chars().peekable();
    let mut out = String::with_capacity(query.len());
    while let Some(ch) = chars.next() {
        out.push(ch);
        if ch == ',' && chars.peek().is_some_and(|c| js_whitespace(*c)) {
            chars.next();
        }
    }
    out
}
fn js_whitespace(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000D}'|'\u{0020}'|'\u{00A0}'|'\u{1680}'|'\u{2000}'..='\u{200A}'|'\u{2028}'|'\u{2029}'|'\u{202F}'|'\u{205F}'|'\u{3000}'|'\u{FEFF}')
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_query_preserves_second_space_and_literal_non_separator_whitespace() {
        assert_eq!(normalize_mpim("Ada,  Grace"), "Ada, Grace");
        assert_eq!(normalize_mpim(" Ada ,\u{00a0}Grace "), " Ada ,Grace ");
        assert_eq!(normalize_mpim("Ada,\u{feff}Grace"), "Ada,Grace");
        assert_eq!(normalize_mpim("Ada,\u{0085}Grace"), "Ada,\u{0085}Grace");
    }
}
