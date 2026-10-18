//! Private result selection for a mounted Profile description. Native caret
//! custody remains in rich_editor.js; rows sent to the view contain no tokens.
use super::*;

const MAX_SAFE: u64 = 9_007_199_254_740_991;

#[derive(Clone)]
pub(super) struct Choices {
    sequence: u64,
    revision: u64,
    consumed: bool,
    source: Option<Arc<canonical::Canonical>>,
    member_source: Option<Arc<crate::directory_member_fetches::search::Snapshot>>,
    group_source: Option<Arc<super::search::group_source::Snapshot>>,
    values: Vec<Completion>,
    cancel: tokio::sync::watch::Sender<bool>,
    ranking_current: Option<Arc<dyn Fn(&ShellState) -> bool + Send + Sync>>,
}

#[derive(Clone, serde::Serialize)]
pub(crate) struct Completion {
    value: Value,
    label: String,
}

#[derive(serde::Serialize)]
pub(crate) struct Presentation {
    pub sequence: u64,
    pub revision: u64,
    pub labels: Vec<String>,
}

/// Host-created identity. Neither a DOM query nor a result row can construct it.
#[derive(Clone)]
pub(crate) struct Lease {
    token: Token,
    visit: u64,
    serial: u64,
    opening: u64,
    key: u64,
    field: String,
    sequence: u64,
    rt_generation: u64,
    cancelled: tokio::sync::watch::Receiver<bool>,
}

impl Lease {
    pub(super) fn member_generation(&self) -> String {
        format!(
            "profile-member-search-{}-{}-{}",
            self.opening, self.key, self.sequence
        )
    }
    pub(super) fn context(&self, shell: &ShellState) -> Option<Arc<crate::scheduled::Context>> {
        self.current(shell)
            .then(|| context(shell, &self.token))
            .flatten()
    }
    pub(super) fn account(&self) -> &str {
        &self.token.owner.account
    }
    pub(super) fn team(&self) -> &str {
        &self.token.owner.team
    }
    pub(super) fn can_dispatch(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && shell
                .profile_edit
                .sections
                .entries
                .get(&self.token.owner)
                .is_none_or(|entry| entry.retry_at <= now())
    }
    pub(super) fn retain_failure(&self, shell: &mut ShellState, error: &Error) {
        let deadline = cooldown(error);
        if deadline > 0 && self.token.current(shell) {
            let entry = shell
                .profile_edit
                .sections
                .entries
                .entry(self.token.owner.clone())
                .or_default();
            entry.retry_at = entry.retry_at.max(deadline);
        }
    }
    /// The task holds only a receiver. Superseding/closing/choosing a query
    /// wakes pending reads; dropping the mounted session closes the channel.
    pub(super) async fn cancelled(&self) {
        let mut receiver = self.cancelled.clone();
        loop {
            if *receiver.borrow_and_update() {
                return;
            }
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        if !editable(shell, self.visit, self.serial)
            || !self.token.current(shell)
            || !current(shell, &self.token, self.visit, self.serial)
            || !shell
                .core
                .workspaces
                .get(&self.token.owner.team)
                .is_some_and(|w| w.rt_generation == self.rt_generation)
        {
            return false;
        }
        shell.profile_edit.sections.draft.as_ref().is_some_and(|d| {
            d.rich_opening == self.opening
                && d.rich.as_ref().is_some_and(|s| {
                    s.key == self.key
                        && s.id == self.field
                        && d.form.as_ref().and_then(|f| f.value(&s.id))
                            == Some(&SectionValue::LongText(s.before.clone()))
                        && s.query
                            .as_ref()
                            .is_some_and(|q| q.sequence == self.sequence && !q.consumed)
                })
        })
    }
}

/// A newer query retires old reads immediately, even before any result exists.
pub(crate) fn begin(
    shell: &mut ShellState,
    visit: u64,
    serial: u64,
    key: u64,
    sequence: u64,
) -> Option<Lease> {
    if !editable(shell, visit, serial) || sequence == 0 || sequence >= MAX_SAFE {
        return None;
    }
    let d = shell.profile_edit.sections.draft.as_ref()?;
    let s = d.rich.as_ref()?;
    if s.key != key
        || s.query.as_ref().is_some_and(|q| q.sequence >= sequence)
        || d.form.as_ref().and_then(|f| f.value(&s.id))
            != Some(&SectionValue::LongText(s.before.clone()))
    {
        return None;
    }
    let (cancel, cancelled) = tokio::sync::watch::channel(false);
    let lease = Lease {
        token: d.token.clone(),
        visit,
        serial,
        opening: d.rich_opening,
        key,
        field: s.id.clone(),
        sequence,
        rt_generation: shell
            .core
            .workspaces
            .get(&d.token.owner.team)?
            .rt_generation,
        cancelled,
    };
    if let Some(previous) = &s.query {
        previous.cancel.send_replace(true);
    }
    shell
        .profile_edit
        .sections
        .draft
        .as_mut()?
        .rich
        .as_mut()?
        .query = Some(Choices {
        sequence,
        revision: 0,
        consumed: false,
        source: None,
        member_source: None,
        group_source: None,
        values: Vec::new(),
        cancel,
        ranking_current: None,
    });
    Some(lease)
}

pub(crate) fn close(shell: &mut ShellState, lease: &Lease) -> bool {
    if !lease.current(shell) {
        return false;
    }
    let q = shell
        .profile_edit
        .sections
        .draft
        .as_mut()
        .unwrap()
        .rich
        .as_mut()
        .unwrap()
        .query
        .as_mut()
        .unwrap();
    q.consumed = true;
    q.values.clear();
    q.source = None;
    q.member_source = None;
    q.group_source = None;
    q.ranking_current = None;
    q.cancel.send_replace(true);
    true
}

/// Called only after Profile search has acquired and ranked canonical IDs.
/// This is label/selection admission, not a search or ranking implementation.
pub(crate) fn publish_channels(
    shell: &mut ShellState,
    lease: &Lease,
    source: Arc<canonical::Canonical>,
    ids: &[String],
) -> Option<Presentation> {
    publish_channels_checked(shell, lease, source, ids, None)
}

/// The matching/ranking controller retains its immutable input witness here;
/// renderer rows never receive or reconstruct this private selection guard.
pub(super) fn publish_channels_checked(
    shell: &mut ShellState,
    lease: &Lease,
    source: Arc<canonical::Canonical>,
    ids: &[String],
    ranking_current: Option<Arc<dyn Fn(&ShellState) -> bool + Send + Sync>>,
) -> Option<Presentation> {
    if !lease.current(shell)
        || !source.current_for(shell, &lease.token.owner.team)
        || ids.len() > 25
        || ranking_current
            .as_ref()
            .is_some_and(|current| !current(shell))
    {
        return None;
    }
    let values = ids
        .iter()
        .map(|id| {
            let value = serde_json::json!({"type":"channel", "channel_id":id});
            let label = canonical::ordinary_label(&value, source.row(id)?).ok()?;
            Some(Completion { value, label })
        })
        .collect::<Option<Vec<_>>>()?;
    let q = shell
        .profile_edit
        .sections
        .draft
        .as_mut()?
        .rich
        .as_mut()?
        .query
        .as_mut()?;
    let revision = q.revision.checked_add(1).filter(|r| *r < MAX_SAFE)?;
    let labels = values.iter().map(|c| c.label.clone()).collect();
    q.revision = revision;
    q.values = values;
    q.source = Some(source);
    q.member_source = None;
    q.group_source = None;
    q.ranking_current = ranking_current;
    Some(Presentation {
        sequence: lease.sequence,
        revision,
        labels,
    })
}

/// Member IDs come only from a ranked batch retaining its eligibility, names,
/// renderer and source observations. Visible labels never carry token authority.
pub(super) fn publish_members_checked(
    shell: &mut ShellState,
    lease: &Lease,
    source: Arc<crate::directory_member_fetches::search::Snapshot>,
    ids: &[String],
    ranking_current: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
) -> Option<Presentation> {
    publish_mixed_checked(
        shell,
        lease,
        source,
        None,
        &ids.iter()
            .cloned()
            .map(MentionId::Member)
            .collect::<Vec<_>>(),
        &std::collections::BTreeMap::new(),
        ranking_current,
    )
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum MentionId {
    Member(String),
    Group(String),
}

/// Both categories share one ranked batch and one private selection receipt.
/// Group display labels are exact NFC observations retained by that batch.
pub(super) fn publish_mixed_checked(
    shell: &mut ShellState,
    lease: &Lease,
    source: Arc<crate::directory_member_fetches::search::Snapshot>,
    groups: Option<Arc<super::search::group_source::Snapshot>>,
    ids: &[MentionId],
    group_labels: &std::collections::BTreeMap<String, String>,
    ranking_current: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
) -> Option<Presentation> {
    if !lease.current(shell)
        || !source.current(shell)
        || groups.as_ref().is_some_and(|source| !source.current(shell))
        || !ranking_current(shell)
        || ids.len() > 25
        || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
    {
        return None;
    }
    let values = ids
        .iter()
        .map(|selection| {
            let id = match selection {
                MentionId::Member(id) => id,
                MentionId::Group(id) => {
                    groups.as_ref()?.rows().iter().find(|row| row.id() == id)?;
                    return Some(Completion {
                        value: serde_json::json!({"type":"usergroup","usergroup_id":id}),
                        label: group_labels.get(id)?.clone(),
                    });
                }
            };
            let row = source
                .rows()
                .iter()
                .find(|r| r.source["id"].as_str() == Some(id.as_str()))?;
            Some(Completion {
                value: serde_json::json!({"type":"user","user_id":id}),
                label: opening::member_label(&row.source).ok()?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let q = shell
        .profile_edit
        .sections
        .draft
        .as_mut()?
        .rich
        .as_mut()?
        .query
        .as_mut()?;
    let revision = q.revision.checked_add(1).filter(|r| *r < MAX_SAFE)?;
    let labels = values.iter().map(|c| c.label.clone()).collect();
    q.revision = revision;
    q.values = values;
    q.source = None;
    q.member_source = Some(source);
    q.group_source = groups;
    q.ranking_current = Some(ranking_current);
    Some(Presentation {
        sequence: lease.sequence,
        revision,
        labels,
    })
}

/// Admission precedes native insertion. A refused native insertion adds no DOM
/// token, and the catalog still cannot authorize fabricated DOM provenance.
pub(crate) fn choose(
    shell: &mut ShellState,
    lease: &Lease,
    revision: u64,
    index: usize,
) -> Option<Completion> {
    if !lease.current(shell) {
        return None;
    }
    let q = shell
        .profile_edit
        .sections
        .draft
        .as_ref()?
        .rich
        .as_ref()?
        .query
        .as_ref()?;
    let source_current = match (&q.source, &q.member_source) {
        (Some(source), None) => {
            q.group_source.is_none() && source.current_for(shell, &lease.token.owner.team)
        }
        (None, Some(source)) => {
            source.current(shell)
                && q.group_source
                    .as_ref()
                    .is_none_or(|groups| groups.current(shell))
        }
        _ => false,
    };
    if !source_current {
        return None;
    }
    if q.ranking_current
        .as_ref()
        .is_some_and(|current| !current(shell))
    {
        return None;
    }
    let s = shell.profile_edit.sections.draft.as_mut()?.rich.as_mut()?;
    // Existing mounted labels are deliberately unchanged. The native insertion
    // remembers the selected occurrence's label independently of initial labels.
    s.query
        .as_mut()?
        .consume(lease.sequence, revision, index, &mut s.catalog)
}

impl Choices {
    fn consume(
        &mut self,
        sequence: u64,
        revision: u64,
        index: usize,
        catalog: &mut catalog::Catalog,
    ) -> Option<Completion> {
        let selected = self.selected(sequence, revision, index)?;
        catalog
            .insert(&selected.value, selected.label.clone())
            .ok()?;
        self.consumed = true;
        self.cancel.send_replace(true);
        Some(selected)
    }
    fn selected(&self, sequence: u64, revision: u64, index: usize) -> Option<Completion> {
        if self.consumed || self.sequence != sequence || revision == 0 || self.revision != revision
        {
            return None;
        }
        self.values.get(index).cloned()
    }
}

#[cfg(test)]
mod tests;

/// Closed native acceptance can identify equal-label rows without exposing
/// selectable tokens in the product DOM or granting an insertion capability.
pub(crate) fn fixture_choices(shell: &ShellState) -> Option<Value> {
    if !fixture::completion::groups::enabled(shell) {
        return None;
    }
    let q = shell
        .profile_edit
        .sections
        .draft
        .as_ref()?
        .rich
        .as_ref()?
        .query
        .as_ref()?;
    if q.consumed {
        return None;
    }
    Some(serde_json::json!({"sequence":q.sequence,"revision":q.revision,"values":q.values}))
}
