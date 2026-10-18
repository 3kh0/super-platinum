//! Presence-preserving cached File/Canvas fields, not authority for navigation.
/// Exact payload fields consumed by the retained mounted File/Document views.
/// No Deserialize: serialized data cannot reconstruct the captured Input.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MountedData {
    #[serde(rename_all = "camelCase")]
    File {
        file_id: String,
        name: Option<String>,
        mime_type: Option<String>,
        deleted: bool,
        file_not_found: bool,
        permalink: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Document {
        file_id: String,
        name: Option<String>,
        rich_text_name: Option<String>,
        should_show_document: bool,
        is_deleted: bool,
        show_badge: bool,
        permalink: Option<String>,
    },
}
