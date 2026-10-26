//! App-defined options have field-local provenance; they are not directory identities.
use crate::{scheduled::Context, state::ShellState};
use dioxus::prelude::*;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use super_platinum_core::{
    block_actions::external::{ExternalOption, ExternalSuggestions},
    slack::{Error, api},
};

#[derive(Clone, Default)]
pub(crate) struct Proofs {
    options: BTreeMap<String, ExternalOption>,
    conflicts: std::collections::BTreeSet<String>,
}
impl Proofs {
    pub(crate) fn remember(&mut self, option: &ExternalOption) {
        if self
            .options
            .get(option.value())
            .is_some_and(|old| old != option)
        {
            self.conflicts.insert(option.value().into());
        } else {
            self.options.insert(option.value().into(), option.clone());
        }
    }
    pub(crate) fn contains(&self, option: &ExternalOption) -> bool {
        !self.conflicts.contains(option.value()) && self.options.get(option.value()) == Some(option)
    }
    pub(crate) fn retain(&mut self, options: impl Iterator<Item = ExternalOption>) {
        let keys: std::collections::BTreeSet<_> = options.map(|o| o.value().to_owned()).collect();
        self.options.retain(|key, _| keys.contains(key));
        self.conflicts.retain(|key| keys.contains(key));
    }
}

/// Retry-After belongs to the account/route, not a disposable picker visit.
#[derive(Default)]
pub(crate) struct Cooldowns {
    entries: Vec<(u64, api::SuggestionsContext, Instant)>,
    overflow: Option<(u64, Instant)>,
}
impl Cooldowns {
    fn deadline(
        &self,
        epoch: u64,
        route: &api::SuggestionsContext,
        now: Instant,
    ) -> Option<Instant> {
        self.entries
            .iter()
            .filter(|(e, r, d)| *e == epoch && r == route && *d > now)
            .map(|(_, _, d)| *d)
            .chain(
                self.overflow
                    .filter(|(e, d)| *e == epoch && *d > now)
                    .map(|(_, d)| d),
            )
            .max()
    }
    fn remember(
        &mut self,
        epoch: u64,
        route: api::SuggestionsContext,
        deadline: Instant,
        now: Instant,
    ) {
        self.entries.retain(|(e, _, d)| *e == epoch && *d > now);
        if self.overflow.is_some_and(|(e, d)| e != epoch || d <= now) {
            self.overflow = None;
        }
        if let Some((_, _, old)) = self
            .entries
            .iter_mut()
            .find(|(e, r, _)| *e == epoch && *r == route)
        {
            *old = (*old).max(deadline);
        } else if self.entries.len() < 256 {
            self.entries.push((epoch, route, deadline));
        } else {
            self.overflow = Some((
                epoch,
                self.overflow.map_or(deadline, |(_, d)| d.max(deadline)),
            ));
        }
    }
}

#[derive(Clone)]
pub(crate) enum Owner {
    Message(super::inputs::external::Ticket),
    Modal(crate::app_modal::external::Ticket),
    Home(crate::app_home::inputs::external::Ticket),
}
impl Owner {
    pub(crate) fn retains_choices(&self) -> bool {
        matches!(self, Self::Home(_))
    }
    pub(crate) fn selected(&self, shell: &ShellState) -> Option<Vec<ExternalOption>> {
        match self {
            Self::Message(t) => t.selected(shell),
            Self::Modal(t) => t.selected(shell),
            Self::Home(t) => t.selected(shell),
        }
    }
    pub(crate) fn id(&self) -> u64 {
        match self {
            Self::Message(t) => t.id(),
            Self::Modal(t) => t.id(),
            Self::Home(t) => t.id(),
        }
    }
    fn current(&self, shell: &ShellState) -> bool {
        match self {
            Self::Message(t) => t.current(shell),
            Self::Modal(t) => t.current(shell),
            Self::Home(t) => t.current(shell),
        }
    }
    fn context(&self) -> Option<Arc<Context>> {
        match self {
            Self::Message(t) => t.context.clone(),
            Self::Modal(t) => t.context.clone(),
            Self::Home(t) => t.context.clone(),
        }
    }
    pub(super) fn route(&self) -> &api::SuggestionsContext {
        match self {
            Self::Message(t) => &t.route,
            Self::Modal(t) => &t.route,
            Self::Home(t) => &t.route,
        }
    }
    pub(crate) fn query<'a>(&self, shell: &'a ShellState) -> Option<&'a Query> {
        if !self.current(shell) {
            return None;
        }
        match self {
            Self::Message(t) => t.query(shell),
            Self::Modal(t) => t.query(shell),
            Self::Home(t) => t.query(shell),
        }
    }
    fn query_mut<'a>(&self, shell: &'a mut ShellState) -> Option<&'a mut Query> {
        if !self.current(shell) {
            return None;
        }
        match self {
            Self::Message(t) => t.query_mut(shell),
            Self::Modal(t) => t.query_mut(shell),
            Self::Home(t) => t.query_mut(shell),
        }
    }
    fn remember(&self, shell: &mut ShellState, options: &[Row]) {
        match self {
            Self::Message(t) => t.remember(shell, options),
            Self::Modal(t) => t.remember(shell, options),
            Self::Home(t) => t.remember(shell, options),
        }
    }
}
impl PartialEq for Owner {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Message(a), Self::Message(b)) => a.id() == b.id(),
            (Self::Modal(a), Self::Modal(b)) => a.same_authority(b),
            (Self::Home(a), Self::Home(b)) => a.same_authority(b),
            _ => false,
        }
    }
}
pub(crate) fn remove(state: Signal<ShellState>, owner: Owner, value: String) {
    match owner {
        Owner::Message(_) => super::inputs::external::remove(state, owner, value),
        Owner::Modal(_) => crate::app_modal::external::remove(state, owner, value),
        Owner::Home(_) => crate::app_home::inputs::external::remove(state, owner, value),
    }
}
pub(crate) fn choose(state: Signal<ShellState>, owner: Owner, revision: u64, index: usize) {
    match owner {
        Owner::Message(_) => super::inputs::external::choose(state, owner, revision, index),
        Owner::Modal(_) => crate::app_modal::external::choose(state, owner, revision, index),
        Owner::Home(_) => crate::app_home::inputs::external::choose(state, owner, revision, index),
    }
}
pub(crate) fn more(mut state: Signal<ShellState>, owner: Owner) {
    if let Some(q) = owner.query_mut(&mut state.write()) {
        q.more();
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub group: Option<String>,
    pub option: ExternalOption,
}
pub(crate) struct Query {
    pub text: String,
    pub revision: u64,
    pub minimum: usize,
    pub rows: Vec<Row>,
    pub loading: bool,
    pub error: Option<String>,
    pub visible: usize,
    cooldown: Option<Instant>,
    cancel: Option<tokio::sync::watch::Sender<bool>>,
}
impl Drop for Query {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl Query {
    pub(crate) fn retire_request(&mut self) {
        self.cancel();
        self.revision = super::next_id();
        self.loading = false;
    }
    pub(crate) fn new(minimum: usize) -> Self {
        Self {
            text: String::new(),
            revision: super::next_id(),
            minimum,
            rows: vec![],
            loading: false,
            error: None,
            visible: 50,
            cooldown: None,
            cancel: None,
        }
    }
    fn cancel(&mut self) {
        if let Some(sender) = self.cancel.take() {
            let _ = sender.send(true);
        }
    }
    pub(crate) fn eligible(&self) -> bool {
        self.text.encode_utf16().count() >= self.minimum && self.text.encode_utf16().count() <= 4096
    }
    pub(crate) fn retryable(&self) -> bool {
        !self.loading
            && self.eligible()
            && self
                .cooldown
                .is_none_or(|deadline| Instant::now() >= deadline)
    }
    pub(crate) fn more(&mut self) {
        self.visible = (self.visible + 50).min(self.rows.len());
    }
}

/// Every edit retires the old request, including edits below the minimum.
pub(crate) fn search(
    mut state: Signal<ShellState>,
    owner: Owner,
    text: String,
    immediate: bool,
    retry: bool,
) {
    let (revision, mut cancellation, cooling) = {
        let mut shell = state.write();
        let retained = shell.message_buttons.external_retry.deadline(
            shell.core.account_epoch,
            owner.route(),
            Instant::now(),
        );
        let Some(query) = owner.query_mut(&mut shell) else {
            return;
        };
        if text == query.text && !retry && (query.loading || !query.rows.is_empty()) {
            return;
        }
        query.cancel();
        query.text = text.clone();
        query.revision = super::next_id();
        if !owner.retains_choices() {
            query.rows.clear();
        }
        query.visible = 50;
        query.error = None;
        query.loading = false;
        query.cooldown = retained.or(query.cooldown);
        if !query.eligible() {
            return;
        }
        let cooling = query.cooldown.filter(|d| Instant::now() < *d);
        if cooling.is_some() {
            query.error = Some("Slack asked us to wait. Retry this search after the delay.".into());
        }
        query.loading = cooling.is_none();
        let (sender, receiver) = tokio::sync::watch::channel(false);
        query.cancel = Some(sender);
        (query.revision, receiver, cooling)
    };
    spawn(async move {
        if let Some(deadline) = cooling {
            tokio::select! { _=tokio::time::sleep(deadline.saturating_duration_since(Instant::now()))=>{}, _=cancellation.changed()=>return }
            if let Some(query) = owner
                .query_mut(&mut state.write())
                .filter(|q| q.revision == revision)
            {
                query.cooldown = None;
            }
            return;
        }
        if !immediate {
            tokio::select! { _=tokio::time::sleep(Duration::from_millis(500))=>{}, _=cancellation.changed()=>return }
        }
        if !owner
            .query(&state.read())
            .is_some_and(|q| q.revision == revision)
        {
            return;
        }
        let request = async {
            if let Some(result) =
                crate::app_home::inputs::external::fixture::suggestions(state, &owner, &text).await
            {
                return result;
            }
            if let Some(result) = super::external_fixture::suggestions(state, &owner, &text).await {
                return result;
            }
            let Some(context) = owner.context() else {
                return Err(Error::Offline("No current connection".into()));
            };
            let request =
                api::block_suggestions(&context.client, &context.session, owner.route(), &text)?;
            context
                .transport
                .execute_block_interaction(request)
                .await
                .and_then(|value| api::decode_external_suggestions(&value))
        };
        let result = tokio::select! { result=request=>result, _=cancellation.changed()=>return };
        complete(&mut state.write(), &owner, revision, result);
        let deadline = owner
            .query(&state.read())
            .filter(|q| q.revision == revision)
            .and_then(|q| q.cooldown);
        if let Some(deadline) = deadline {
            tokio::select! { _=tokio::time::sleep(deadline.saturating_duration_since(Instant::now()))=>{}, _=cancellation.changed()=>return }
            if let Some(query) = owner
                .query_mut(&mut state.write())
                .filter(|q| q.revision == revision)
            {
                query.cooldown = None;
            }
        }
    });
}
pub(crate) fn complete(
    shell: &mut ShellState,
    owner: &Owner,
    revision: u64,
    result: Result<ExternalSuggestions, Error>,
) -> bool {
    if !owner.query(shell).is_some_and(|q| q.revision == revision) {
        return false;
    }
    match result {
        Ok(options) => {
            let rows = match options {
                ExternalSuggestions::Options(options) => options
                    .into_iter()
                    .map(|option| Row {
                        group: None,
                        option,
                    })
                    .collect::<Vec<_>>(),
                ExternalSuggestions::Groups(groups) => groups
                    .into_iter()
                    .flat_map(|group| {
                        group.options.into_iter().map(move |option| Row {
                            group: Some(group.label.clone()),
                            option,
                        })
                    })
                    .collect(),
            };
            owner.remember(shell, &rows);
            let query = owner.query_mut(shell).unwrap();
            query.rows = rows;
            // Retained Home rows and replacement rows must not share a click identity.
            if owner.retains_choices() {
                query.revision = super::next_id();
            }
            query.loading = false;
            query.error = None;
        }
        Err(error) => {
            let query = owner.query_mut(shell).unwrap();
            query.loading = false;
            query.error=Some(match error {
                Error::RateLimited{retry_after_secs,..}=>{
                    query.cooldown=Instant::now().checked_add(Duration::from_secs(retry_after_secs.unwrap_or(5).max(1))).or_else(||Instant::now().checked_add(Duration::from_secs(3_155_760_000)));
                    "Slack asked us to wait. This search was not repeated.".into()
                }
                Error::Offline(_)=>"Options could not be loaded while disconnected. Retry when connected.".into(),
                _=>"The app's options could not be confirmed. This search was not repeated; you can retry explicitly.".into(),
            });
        }
    }
    if let Some(deadline) = owner.query(shell).and_then(|q| q.cooldown) {
        shell.message_buttons.external_retry.remember(
            shell.core.account_epoch,
            owner.route().clone(),
            deadline,
            Instant::now(),
        );
    }
    true
}
pub(crate) fn context_current(shell: &ShellState, context: &Option<Arc<Context>>) -> bool {
    context
        .as_ref()
        .map_or(shell.core.transport.is_none(), |c| {
            super::inputs::user::directory::context_current(shell, c)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retiring_value_bound_request_keeps_results_and_cooldown_but_rejects_old_revision() {
        let mut query = Query::new(0);
        query.text = "release".into();
        query.loading = true;
        query.cooldown = Some(Instant::now() + Duration::from_secs(30));
        let deadline = query.cooldown;
        let option = ExternalOption::parse(
            &serde_json::json!({"text":{"type":"plain_text","text":"A"},"value":"a"}),
        )
        .unwrap();
        query.rows.push(Row {
            group: Some("Releases".into()),
            option: option.clone(),
        });
        let old_revision = query.revision;
        let (sender, receiver) = tokio::sync::watch::channel(false);
        query.cancel = Some(sender);
        query.retire_request();
        assert_ne!(query.revision, old_revision);
        assert!(!query.loading);
        assert!(*receiver.borrow());
        assert_eq!(query.text, "release");
        assert_eq!(query.rows[0].option, option);
        assert_eq!(query.cooldown, deadline);
        assert!(!query.retryable());
    }
    #[test]
    fn utf16_threshold_and_exact_raw_proofs() {
        let mut q = Query::new(3);
        q.text = "😀a".into();
        assert!(q.eligible());
        q.text = "😀".into();
        assert!(!q.eligible());
        let a = ExternalOption::parse(
            &serde_json::json!({"text":{"type":"plain_text","text":"A"},"value":"same"}),
        )
        .unwrap();
        let b = ExternalOption::parse(
            &serde_json::json!({"text":{"type":"plain_text","text":"B"},"value":"same"}),
        )
        .unwrap();
        let mut proofs = Proofs::default();
        proofs.remember(&a);
        assert!(proofs.contains(&a));
        proofs.remember(&b);
        assert!(!proofs.contains(&a));
        assert!(!proofs.contains(&b));
    }
    #[test]
    fn retry_after_survives_visits_extends_and_expires_without_replay() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        crate::app_modal::external::install_fixture(&mut shell);
        let s = shell.app_modal.as_ref().unwrap();
        let route = api::SuggestionsContext::modal(
            s.modal.scope(),
            "U1",
            s.modal.stack().last().unwrap(),
            &super_platinum_core::block_actions::form::InputKey {
                block_id: "choice".into(),
                action_id: "pick".into(),
            },
        )
        .unwrap();
        let now = Instant::now();
        let mut cooldowns = Cooldowns::default();
        cooldowns.remember(1, route.clone(), now + Duration::from_secs(3), now);
        cooldowns.remember(1, route.clone(), now + Duration::from_secs(1), now);
        assert_eq!(
            cooldowns.deadline(1, &route, now),
            Some(now + Duration::from_secs(3))
        );
        assert!(cooldowns.deadline(2, &route, now).is_none());
        let mut reopened = Query::new(0);
        reopened.cooldown = cooldowns.deadline(1, &route, now);
        assert!(!reopened.retryable());
        assert!(
            cooldowns
                .deadline(1, &route, now + Duration::from_secs(3))
                .is_none()
        );
        // Expiry changes eligibility only; it does not enqueue any request.
        reopened.cooldown = None;
        assert!(reopened.retryable());
        assert!(!reopened.loading);
        assert!(reopened.rows.is_empty());
    }
    #[test]
    fn editing_and_drop_retire_the_request_receiver() {
        let mut q = Query::new(3);
        let (tx, rx) = tokio::sync::watch::channel(false);
        q.cancel = Some(tx);
        q.cancel();
        assert!(*rx.borrow());
        let (tx, rx) = tokio::sync::watch::channel(false);
        q.cancel = Some(tx);
        drop(q);
        assert!(*rx.borrow());
    }
}
