//! Same-ID Undo/Redo uses updateDeletion, never records.create or cell writes.
use super::*;

#[derive(Debug)]
pub struct DeletionStateIntent {
    document: Document,
    original: Record,
    before: ListRecordReadback,
    deleted: bool,
    redo: Option<DeletionIntent>,
}

impl DeletionIntent {
    /// Caller must retain this original intent under its account ownership and
    /// require a new explicit Undo/Redo gesture. An exact readback establishes
    /// the starting state, not which client caused it. No timer grants consent.
    #[allow(clippy::too_many_arguments)]
    pub fn deletion_state_intent(
        &self,
        document: &Document,
        page: &RecordsPage,
        readback: &ListRecordReadback,
        deleted: bool,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<DeletionStateIntent, ListError> {
        permission(document, self.list_id(), allow_lists, personal)?;
        if readback.list_id() != self.list_id()
            || readback.record_id() != self.record_id()
            || readback.is_deleted() == deleted
        {
            return Err(ListError(
                "This item's deletion state changed. Check it again.",
            ));
        }
        let redo = if deleted {
            let current = readback
                .record()
                .ok_or(ListError("This item is unavailable."))?;
            if !same_identity(&self.record, current, self.list_id())
                || page
                    .record(self.record_id())
                    .is_none_or(|record| !same_page_record(record, current))
            {
                return Err(ListError("This item no longer matches the deleted item."));
            }
            Some(Self::capture(
                document,
                page,
                self.record_id(),
                allow_lists,
                personal,
            )?)
        } else {
            None
        };
        Ok(DeletionStateIntent {
            document: document.clone(),
            original: self.record.clone(),
            before: readback.clone(),
            deleted,
            redo,
        })
    }
}

impl DeletionStateIntent {
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn record_id(&self) -> &str {
        &self.original.id
    }
    pub fn deleted(&self) -> bool {
        self.deleted
    }

    pub fn revalidate(
        &self,
        document: &Document,
        page: &RecordsPage,
        readback: &ListRecordReadback,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        permission(document, self.list_id(), allow_lists, personal)?;
        if !super::super::cell_edit::document_matches(&document.raw, &self.document.raw)
            || readback != &self.before
        {
            return Err(ListError("This item changed. Review Undo or Redo again."));
        }
        if let Some(redo) = &self.redo {
            redo.revalidate(document, page, allow_lists, personal)?;
        }
        Ok(())
    }

    pub fn validate_response(&self, response: &Value) -> Result<AcceptedDeletionState, ListError> {
        if response["ok"] != true
            || response.get("error").is_some_and(|v| !v.is_null())
            || conflicts(response, "list_id", self.list_id())
            || conflicts(response, "id", self.record_id())
            || response
                .get("deleted")
                .is_some_and(|value| value.as_bool() != Some(self.deleted))
        {
            return Err(ListError(
                "Slack has not confirmed this item's deletion-state change.",
            ));
        }
        Ok(AcceptedDeletionState {
            list_id: self.list_id().into(),
            original: self.original.clone(),
            deleted: self.deleted,
        })
    }

    /// An uncertain response can be reconciled by exact target state. This is
    /// not proof that this particular request caused the observed transition.
    pub fn confirms_readback(&self, readback: &ListRecordReadback) -> bool {
        confirms(&self.document.id, &self.original, self.deleted, readback)
    }
}

#[derive(Clone, Debug)]
pub struct AcceptedDeletionState {
    list_id: String,
    original: Record,
    deleted: bool,
}
impl AcceptedDeletionState {
    pub fn list_id(&self) -> &str {
        &self.list_id
    }
    pub fn record_id(&self) -> &str {
        &self.original.id
    }
    pub fn deleted(&self) -> bool {
        self.deleted
    }
    pub fn confirms_readback(&self, readback: &ListRecordReadback) -> bool {
        confirms(&self.list_id, &self.original, self.deleted, readback)
    }
}

fn permission(
    document: &Document,
    list_id: &str,
    allow: Option<&str>,
    personal: Option<&PersonalTaskLists>,
) -> Result<(), ListError> {
    if document.id != list_id || !valid_id(list_id) {
        return Err(ListError("This List identity changed."));
    }
    if let Some(reason) = document.read_only_reason(allow) {
        return Err(ListError(reason));
    }
    if personal.is_none_or(|policy| policy.contains(list_id)) {
        return Err(ListError(
            "Ordinary List deletion preferences are not verified.",
        ));
    }
    Ok(())
}
fn confirms(
    list_id: &str,
    original: &Record,
    deleted: bool,
    readback: &ListRecordReadback,
) -> bool {
    readback.list_id() == list_id
        && readback.record_id() == original.id
        && if deleted {
            readback.is_deleted()
        } else {
            readback
                .record()
                .is_some_and(|record| same_identity(original, record, list_id))
        }
}
fn same_identity(original: &Record, current: &Record, list_id: &str) -> bool {
    validate_record(current, list_id).is_ok()
        && current.id == original.id
        // S iLfW decodes parent_record_id ?? ""; missing, null and empty
        // therefore all identify a root. Nonempty parents still match exactly.
        && current.parent_id() == original.parent_id()
        && ["date_created", "created_by"]
            .iter()
            .all(|key| current.raw.get(*key) == original.raw.get(*key))
}

/// records.info explicitly requests is_subscribed; records.list can omit it.
/// This read-state metadata never authorizes a content change. Root parent
/// aliases follow the same captured nullish decoder used by identity matching.
fn same_page_record(page: &Record, info: &Record) -> bool {
    let (Some(page_raw), Some(info_raw)) = (page.raw.as_object(), info.raw.as_object()) else {
        return false;
    };
    let retained = |key: &str| key != "is_subscribed" && key != "parent_record_id";
    page.parent_id() == info.parent_id()
        && page_raw.keys().filter(|key| retained(key)).count()
            == info_raw.keys().filter(|key| retained(key)).count()
        && page_raw
            .iter()
            .filter(|(key, _)| retained(key))
            .all(|(key, value)| info_raw.get(key) == Some(value))
}

#[cfg(test)]
mod tests;
