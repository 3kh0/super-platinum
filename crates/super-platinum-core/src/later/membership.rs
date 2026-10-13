//! Saved membership is independent of the visible filter and pagination. A
//! missing list entry or omitted lookup response never proves absence.
use super::*;

#[derive(Clone, Copy, Debug)]
pub enum SavedMembership<'a> {
    Unknown,
    Present(&'a SavedItem),
    /// Only a positively confirmed delete or saved_deleted event sets this.
    Absent,
}
#[derive(Clone, Debug)]
pub struct SavedRequest {
    scope: (u64, String),
    key: SavedKey,
    revision: u64,
    scope_revision: u64,
}
impl SavedRequest {
    pub fn key(&self) -> &SavedKey {
        &self.key
    }
}
impl LaterState {
    pub fn membership(&self, key: &SavedKey) -> SavedMembership<'_> {
        match self.membership.get(key) {
            Some(Some(item)) => SavedMembership::Present(item),
            Some(None) => SavedMembership::Absent,
            None => SavedMembership::Unknown,
        }
    }
    fn next_item_revision(&mut self, key: &SavedKey) -> u64 {
        self.item_revision = self
            .item_revision
            .checked_add(1)
            .expect("saved-item revision exhausted");
        self.item_revisions.insert(key.clone(), self.item_revision);
        self.item_revision
    }
    pub(super) fn record_membership(&mut self, key: &SavedKey, item: Option<SavedItem>) {
        self.next_item_revision(key);
        self.membership.insert(key.clone(), item);
    }
    /// Capture immediately before the read/write. This authorizes no network
    /// mutation and owns no busy flag. A newer request or item observation
    /// supersedes this token; unrelated items do not.
    pub fn begin_item_request(&mut self, key: SavedKey) -> Option<SavedRequest> {
        let scope = self.scope.clone()?;
        if key.item_id.is_empty()
            || key.item_type.is_empty()
            || (key.item_type == "file" && !key.ts.is_empty())
        {
            return None;
        }
        // A list read begun before this explicit request cannot renew stale
        // membership and thereby reject its later positive response. A newer
        // list request still gets its own generation and may supersede it.
        self.generation = self.generation.wrapping_add(1);
        self.loading = false;
        let revision = self.next_item_revision(&key);
        Some(SavedRequest {
            scope,
            key,
            revision,
            scope_revision: self.scope_revision,
        })
    }
    /// None means a positively confirmed delete, NEVER an omitted targeted
    /// lookup or an absent filtered/paginated row. The adapter validates this
    /// protocol evidence before calling. Busy state remains controller-owned.
    pub fn apply_item_response(&mut self, request: &SavedRequest, item: Option<SavedItem>) -> bool {
        if self.scope.as_ref() != Some(&request.scope)
            || self.scope_revision != request.scope_revision
            || self.item_revisions.get(&request.key) != Some(&request.revision)
            || item.as_ref().is_some_and(|item| item.key() != request.key)
        {
            return false;
        }
        self.updated(&request.key, item);
        true
    }
}

#[cfg(test)]
mod tests;
