//! Reuse the owning REST URL, token-first fields and desktop identity. The
//! closed avatar executor supplies multipart Content-Type with its boundary.
use super::*;

impl SlackClient {
    pub(crate) fn avatar_multipart(
        &self,
        workspace: &WorkspaceSession,
        endpoint: &str,
        fields: Vec<(&str, String)>,
    ) -> PreparedRequest {
        let mut request = self.rest_form(workspace, endpoint, fields);
        request
            .headers
            .retain(|(key, _)| !key.eq_ignore_ascii_case("content-type"));
        request
    }
}
