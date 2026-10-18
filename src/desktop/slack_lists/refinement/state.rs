//! Visit-owned read refinements; canonical Loaded data and provider stay separate.
use super::*;
use super_platinum_core::slack_lists::{ListError, ViewRefinementChanges, ViewRefinementDraft};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::slack_lists) struct State {
    drafts: BTreeMap<String, ViewRefinementDraft>,
}
impl State {
    pub fn is_empty(&self) -> bool {
        self.drafts.is_empty()
    }
    pub fn all_document(&self, document: &Document) -> Result<Document, ListError> {
        let mut effective = document.clone();
        for (id, draft) in &self.drafts {
            // A removed view's retained local slot cannot hide other readable
            // views. It is still reset with the complete visit map.
            if effective.views.iter().any(|v| v["id"] == *id) {
                effective = draft.projection_document(&effective)?;
            }
        }
        Ok(effective)
    }
    pub fn draft(&self, document: &Document, id: &str) -> Result<ViewRefinementDraft, ListError> {
        match self.drafts.get(id) {
            Some(draft) => {
                draft.projection_document(document)?;
                Ok(draft.clone())
            }
            None => ViewRefinementDraft::capture(document, id),
        }
    }
    pub fn document(&self, document: &Document, id: &str) -> Result<Document, ListError> {
        self.draft(document, id)?.projection_document(document)
    }
    pub fn changes(
        &self,
        document: &Document,
        id: &str,
    ) -> Result<ViewRefinementChanges, ListError> {
        self.draft(document, id)?.changes(document)
    }
    pub fn replace(&mut self, draft: ViewRefinementDraft) -> bool {
        if self.drafts.get(draft.view_id()) == Some(&draft) {
            return false;
        }
        self.drafts.insert(draft.view_id().into(), draft);
        true
    }
    /// Mirrors AZAI Reset's full map clear within this native List visit.
    /// The separate Layout.board_order provider is deliberately not an argument.
    pub fn reset(&mut self) -> bool {
        if self.drafts.is_empty() {
            return false;
        }
        self.drafts.clear();
        true
    }
}
