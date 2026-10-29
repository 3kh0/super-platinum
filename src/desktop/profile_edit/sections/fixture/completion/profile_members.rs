//! A named SDK recipe; the original fixture grammar stays closed and unchanged.
use super::*;

pub(super) fn validate(
    client: &SlackClient,
    owner: &WorkspaceSession,
    request: &PreparedRequest,
) -> Result<Accepted, Error> {
    let RequestBody::Json(body) = &request.body else {
        return Err(refused());
    };
    if body.get("include_profile_only_users") != Some(&Value::Bool(true)) {
        return Err(refused());
    }
    let query = super_platinum_core::slack::api::custom::texty::members::Query::capture(
        owner,
        body["query"].as_str().ok_or_else(refused)?,
        super_platinum_core::slack::api::custom::texty::members::RequestFacts {
            top_users: Some(json!([])),
            default_workspace: Some(owner.team_id.clone()),
        },
    )?;
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(owner, &discovery(owner), now)?;
    let expected = query.request_for_profile_long_text(client, owner, &endpoint, now, None)?;
    if !same_request(request, &expected) {
        return Err(refused());
    }
    let mut legacy = request.clone();
    let RequestBody::Json(body) = &mut legacy.body else {
        return Err(refused());
    };
    body.as_object_mut()
        .ok_or_else(refused)?
        .remove("include_profile_only_users");
    let mut accepted = super::validate(client, owner, &legacy)?;
    // Keep the actual SDK arguments in the redacted ledger, not the legacy clone.
    accepted.args = redacted(request)?;
    Ok(accepted)
}

#[cfg(test)]
mod tests;
