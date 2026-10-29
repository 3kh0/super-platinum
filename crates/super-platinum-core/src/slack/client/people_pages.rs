//! Closed read-only People method. Generic Form/JSON encoding stays unchanged.
use super::*;

impl SlackClient {
    pub(in crate::slack) fn people_page_multipart(
        &self,
        workspace: &WorkspaceSession,
        fields: Vec<(&str, String)>,
    ) -> PreparedRequest {
        let mut request = self.rest_form(workspace, "search.modules.people", fields);
        request
            .headers
            .retain(|(key, _)| !key.eq_ignore_ascii_case("content-type"));
        request
    }
}
