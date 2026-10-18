//! Captured 2gUC paging and 6OO5 channel normalization, without selection authority.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone)]
pub struct DirectoryResults {
    directory: Directory,
    pages: Vec<DirectoryPage>,
    rows: Vec<Value>,
    members: Vec<String>,
    seen_markers: BTreeSet<String>,
    marker: Option<String>,
    complete: bool,
}
impl DirectoryResults {
    pub fn new(directory: Directory) -> Self {
        Self {
            directory,
            pages: Vec::new(),
            rows: Vec::new(),
            members: Vec::new(),
            seen_markers: BTreeSet::new(),
            marker: None,
            complete: false,
        }
    }
    pub fn directory(&self) -> Directory {
        self.directory
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn request_marker(&self) -> Option<&str> {
        self.marker.as_deref()
    }
    pub fn pages(&self) -> &[DirectoryPage] {
        &self.pages
    }
    pub fn rows(&self) -> &[Value] {
        &self.rows
    }
    pub fn append(&mut self, page: DirectoryPage) -> Result<(), Error> {
        if self.complete || self.pages.len() >= 50 {
            return Err(invalid());
        }
        let limit = if self.directory == Directory::Users {
            24
        } else {
            100
        };
        let count = self.rows.len() + page.rows().len();
        let complete = page.next_marker().is_none() || count >= limit;
        let marker = if count > limit {
            page.rows()
                .get(limit.saturating_sub(self.rows.len()))
                .and_then(|v| v["id"].as_str())
                .map(str::to_owned)
        } else {
            page.next_marker().map(str::to_owned)
        };
        // This is a bounded transport refusal, not permission to present a partial
        // result as the completed captured query. Slack's ordinary loop has no cap.
        if !complete
            && (self.pages.len() == 49
                || marker
                    .as_ref()
                    .is_some_and(|m| self.seen_markers.contains(m)))
        {
            return Err(invalid());
        }
        if let Some(marker) = &marker {
            self.seen_markers.insert(marker.clone());
        }
        self.members.extend_from_slice(page.member_channels());
        self.rows.extend(
            page.rows()
                .iter()
                .take(limit.saturating_sub(self.rows.len()))
                .cloned(),
        );
        self.pages.push(page);
        self.complete = complete;
        self.marker = marker;
        Ok(())
    }
    /// The actual raw search row remains available in `rows`/`pages`. This
    /// normalization grants neither posting permission nor a current owner.
    pub fn conversation(&self, index: usize, cached: Option<&Value>) -> Result<Value, Error> {
        if !self.complete {
            return Err(invalid());
        }
        let row = self.rows.get(index).ok_or_else(invalid)?;
        match self.directory {
            Directory::Users => Err(invalid()),
            Directory::Channels => {
                let mut channel = row.clone();
                if !self.members.is_empty() {
                    channel["is_member"] = json!(
                        self.members.iter().any(|m| row["id"] == *m)
                            && (row["is_archived"] != true
                                || ((row["is_private"] == true || row["is_group"] == true)
                                    && row["is_mpim"] != true))
                    );
                }
                Ok(channel)
            }
            Directory::GroupMessages => {
                let key = row["id"].as_str().ok_or_else(invalid)?;
                if cached.is_some_and(|v| v["id"] != key || v["is_mpim"] == false) {
                    return Err(invalid());
                }
                let members = row
                    .get("members")
                    .and_then(Value::as_array)
                    .filter(|m| m.len() <= 9)
                    .ok_or_else(invalid)?;
                if members.iter().any(|v| v.as_str().is_none_or(|s| !id(s))) {
                    return Err(invalid());
                }
                let details = row.get("details").filter(|v| v.is_object());
                if details.is_some_and(|v| v.get("id").is_some_and(|id| id != key))
                    || (!cached.is_some_and(|v| v["is_mpim"] == true)
                        && !details.is_some_and(|v| v["is_mpim"] == true))
                {
                    return Err(invalid());
                }
                let mut channel = cached
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                if let Some(details) = details.and_then(Value::as_object) {
                    channel.extend(details.clone());
                }
                // Retained details/cache must identify this exact result. Never
                // substitute the outer row's name or manufacture a destination.
                if channel.get("id").and_then(Value::as_str) != Some(key) {
                    return Err(invalid());
                }
                channel.extend(json!({
                    "members":members,"internal_team_ids":[],"is_archived":false,
                    "is_channel":true,"is_general":false,"is_global_shared":true,
                    "is_group":false,"is_im":false,"is_moved":0,"is_member":true,
                    "is_mpim":true,"is_org_default":false,"is_org_mandatory":false,
                    "is_org_shared":true,"is_pending_ext_shared":false,"is_private":true,
                    "parent_conversation":null,"purpose":{},"connected_limited_team_ids":[],
                    "is_open":cached.and_then(|v|v.get("is_open")).filter(|v|!v.is_null()).cloned().unwrap_or(json!(false))
                }).as_object().unwrap().clone());
                Ok(Value::Object(channel))
            }
        }
    }
}
