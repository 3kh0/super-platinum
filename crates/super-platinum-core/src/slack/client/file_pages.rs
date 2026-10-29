//! Closed private Files text multipart builder; generic encodings stay intact.
use super::*;
impl SlackClient {
    pub(in crate::slack) fn file_page_multipart(
        &self,
        workspace: &WorkspaceSession,
        fields: Vec<(&str, String)>,
    ) -> PreparedRequest {
        let mut request = self.rest_form(workspace, "search.modules.files", fields);
        request
            .headers
            .retain(|(key, _)| !key.eq_ignore_ascii_case("content-type"));
        request
    }
}
