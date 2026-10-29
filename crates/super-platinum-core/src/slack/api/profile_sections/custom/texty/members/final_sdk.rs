//! euu1's final SDK default, separate from the retained WNgx argument recipe.
use super::*;
use crate::slack::client::RequestBody;

impl Query {
    pub fn request_for_profile_long_text(
        &self,
        client: &SlackClient,
        owner: &WorkspaceSession,
        endpoint: &DirectoryEndpoint,
        now: Instant,
        marker: Option<&str>,
    ) -> Result<PreparedRequest, Error> {
        let mut request = self.request(client, owner, endpoint, now, marker)?;
        let RequestBody::Json(body) = &mut request.body else {
            return Err(unavailable());
        };
        body.as_object_mut()
            .ok_or_else(unavailable)?
            .insert("include_profile_only_users".into(), Value::Bool(true));
        Ok(request)
    }
}

#[cfg(test)]
mod tests;
