//! Reaction syntax needs no cached identity choice. Keep manual fallback fields
//! while owning their entire native Messages draft and queued read frame.
use super::*;

pub(crate) struct Draft {
    origin: Arc<Origin>,
    edit: ScopeEdit,
}
#[derive(Clone)]
pub(crate) struct Source {
    origin: Arc<Origin>,
}
pub(crate) fn has_draft(s: &ShellState) -> bool {
    s.search_message_fallback_filters.is_some()
}
impl Source {
    pub(crate) fn capture(s: &ShellState) -> Option<Self> {
        let source = Self {
            origin: s.search_message_fallback_filters.as_ref()?.origin.clone(),
        };
        source.current(s).then_some(source)
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        s.signed_in
            && !s.loading
            && self.origin.owner.current(s)
            && generic_allowed(s)
            && s.search_file_type_filters.is_none()
            && !s.search_history_open
            && s.search_filters
                .as_ref()
                .is_some_and(|f| f.reactions.is_some())
            && s.search_message_fallback_filters
                .as_ref()
                .is_some_and(|d| Arc::ptr_eq(&d.origin, &self.origin))
    }
    pub(crate) fn key(&self) -> &str {
        &self.origin.key
    }
}

/// Called only after the original toggle actually changed panel/history state.
/// A rejected toggle cannot acquire a new owner for an existing draft.
pub(crate) fn after_toggle(s: &mut ShellState) {
    let fields = s.search_filters.take();
    s.cancel_search_open();
    s.search_filters = fields;
    if !s.signed_in
        || s.loading
        || !generic_allowed(s)
        || s.search_pager.kind != SearchKind::Messages
        || s.search_history_open
        || s.search_filters.is_none()
        || s.search_file_type_filters.is_some()
    {
        return;
    }
    let Some(owner) = Owner::capture(s) else {
        return;
    };
    let Ok(edit) = ScopeEdit::capture(&s.search_query, Vec::new()) else {
        return;
    };
    // Only the new scalar intent uses the opaque-aware editor. The existing
    // generic In/From/basic controls retain their manual syntax and semantics.
    s.search_filters.as_mut().unwrap().reactions = edit.fields().reactions;
    s.search_message_fallback_filters = Some(Draft {
        origin: Origin::new(owner),
        edit,
    });
}

pub(crate) fn apply(s: &mut ShellState, source: &Source) -> Option<Submission> {
    if !source.current(s) {
        return None;
    }
    let d = s.search_message_fallback_filters.as_ref().unwrap();
    let fields = s.search_filters.as_ref().unwrap();
    let mut reactions = d.edit.fields();
    reactions.reactions = fields.reactions.clone();
    // Validate new reaction intent against the ORIGINAL lexical frame first.
    // Generic edits cannot erase an unfinished quote/reference before that check.
    let result = d.edit.apply(&s.search_query, &reactions).and_then(|query| {
        let mut legacy = fields.clone();
        legacy.reactions = None;
        legacy.apply(&query)
    });
    match result {
        Ok(query) => {
            let unchanged = query == s.search_query;
            let mut owner = source.origin.owner.clone();
            s.replace_search_query(query);
            s.search_filters = None;
            s.search_filter_error = None;
            owner.rebase_query(s);
            owner.current(s).then_some(Submission { owner, unchanged })
        }
        Err(error) => {
            s.search_filter_error = Some(error);
            None
        }
    }
}

/// The generic panel's Clear has always submitted immediately. Preserve it,
/// including explicit removal of owned reaction clauses, under the same origin.
pub(crate) fn clear(s: &mut ShellState, source: &Source) -> Option<Submission> {
    if !source.current(s) {
        return None;
    }
    s.search_filters = Some(SearchFilters {
        reactions: Some(Default::default()),
        ..Default::default()
    });
    apply(s, source)
}
