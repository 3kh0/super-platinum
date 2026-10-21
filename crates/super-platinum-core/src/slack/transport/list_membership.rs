//! Two closed List Texty membership reads, using the existing ordered decoder
//! and link/current/error policy. No credentials or network capability added.
use super::*;
use crate::slack::api::custom::texty::list_membership::valid_wire_body;

#[cfg(test)]
mod tests;

impl Transport {
    pub async fn execute_list_membership_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<ListMembershipResponse, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_list_membership_request".into()));
        }
        self.execute_profile_member_read_admitted(req, current)
            .await
    }
}

fn allowed(req: &PreparedRequest) -> bool {
    if req.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&req.url) else {
        return false;
    };
    let parts: Vec<_> = url.path().split('/').collect();
    if url.scheme() != "https"
        || url.host_str() != Some("edgeapi.slack.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || parts.len() != 5
        || parts[1] != "cache"
        || parts[2].is_empty()
        || parts[2].len() > 256
        || !matches!(parts[2].as_bytes().first(), Some(b'T' | b'E'))
        || !parts[2]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return false;
    }
    let RequestBody::Json(body) = &req.body else {
        return false;
    };
    let method = match (parts[3], parts[4]) {
        ("users", "list") => "users/list",
        ("channels", "membership") => "channels/membership",
        _ => return false,
    };
    valid_wire_body(method, body)
}
