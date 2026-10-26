//! The distinct, nonpersistent Canvas section-rich-text read (7nvy/yk2R).
//! These responses are not collaboration Sections or file metadata.
use super::*;
use serde_json::Value;

#[derive(Clone)]
pub struct CanvasSectionsRequest {
    pub(crate) identity: Arc<()>,
    file: String,
    sections: Vec<String>,
}
#[derive(Clone)]
pub struct CanvasSectionsResponse {
    pub(crate) request: CanvasSectionsRequest,
    pub(crate) rows: Option<Vec<(String, Option<Value>)>>,
}
fn invalid() -> Error {
    Error::Api("invalid_canvas_section_reference".into())
}
pub(crate) fn section_identity(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        // Canonical Section identity is opaque (including temp:C:...). Comma
        // alone is additionally excluded because section_ids is comma-joined.
        && !id.chars().any(|c| c.is_control() || c == ',')
}
pub(crate) fn section_file_identity(id: &str) -> bool {
    id.starts_with('F')
        && id.len() > 1
        && id.len() <= 256
        && id.bytes().all(|b| b.is_ascii_alphanumeric())
}
impl CanvasSectionsRequest {
    pub fn new(file: &str, sections: &[String]) -> Result<Self, Error> {
        if !section_file_identity(file)
            || sections.is_empty()
            || sections.len() > 1000
            || sections.iter().any(|id| !section_identity(id))
        {
            return Err(invalid());
        }
        Ok(Self {
            identity: Arc::new(()),
            file: file.into(),
            sections: sections.to_vec(),
        })
    }
    pub fn file_id(&self) -> &str {
        &self.file
    }
    pub fn section_ids(&self) -> &[String] {
        &self.sections
    }
    /// `reason` is diagnostic facade data, not a wire parameter. The original
    /// FormData encoder joins this string array with commas.
    pub fn prepare(&self, client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
        client.rest_form(
            workspace,
            "quip.getCanvasBulkSectionIdsData",
            vec![
                ("file_id", self.file.clone()),
                ("section_ids", self.sections.join(",")),
            ],
        )
    }
    /// Decode at the actual read boundary. A malformed later entry has the same
    /// final publication as the original catch: clear this file, including any
    /// earlier entry in that response. Omission/empty sections are no-ops.
    pub fn decode(&self, response: Value) -> CanvasSectionsResponse {
        let rows = (|| {
            if response.get("ok").and_then(Value::as_bool) != Some(true)
                || serde_json::to_vec(&response).ok()?.len() > 2 * 1024 * 1024
            {
                return None;
            }
            let sections = match response.get("sections") {
                None | Some(Value::Null) => return Some(Vec::new()),
                Some(Value::Bool(false)) => return Some(Vec::new()),
                Some(Value::String(value)) if value.is_empty() => return Some(Vec::new()),
                Some(Value::Number(value)) if value.as_f64() == Some(0.0) => {
                    return Some(Vec::new());
                }
                Some(Value::Array(rows)) if rows.len() <= 1000 => rows,
                _ => return None,
            };
            let mut result = Vec::with_capacity(sections.len());
            for section in sections {
                let id = section
                    .get("id")?
                    .as_str()
                    .filter(|id| section_identity(id))?;
                let blocks = section.get("rich_text_blocks")?.as_array()?;
                // An empty array creates an own key with undefined. Keep that
                // separate from both an absent key and an explicit null block.
                result.push((id.to_owned(), blocks.first().cloned()));
            }
            Some(result)
        })();
        CanvasSectionsResponse {
            request: self.clone(),
            rows,
        }
    }
    /// The caller invokes this only for this owned request's caught read error.
    pub fn failed(&self) -> CanvasSectionsResponse {
        CanvasSectionsResponse {
            request: self.clone(),
            rows: None,
        }
    }
}
impl CanvasSectionsResponse {
    pub fn file_id(&self) -> &str {
        self.request.file_id()
    }
    pub fn clears_file(&self) -> bool {
        self.rows.is_none()
    }
}

#[cfg(test)]
mod tests;
