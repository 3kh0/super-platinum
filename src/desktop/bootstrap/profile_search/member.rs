//! Purpose-specific read-only member pane. No canonical user or action conversion.
use super::*;
use serde_json::Value;
use super_platinum_core::slack::{SlackClient, Transport, client::PreparedRequest};

#[derive(Clone)]
pub(super) struct Owner {
    team: String,
    read: Arc<crate::bootstrap::ReadOwner>,
    client: SlackClient,
    origin: Option<Arc<crate::thread_feed::ProfileMember>>,
    search: Option<Arc<crate::bootstrap::search_message_pages::profiles::Pane>>,
}
impl Owner {
    pub(super) fn team(&self) -> &str {
        &self.team
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_team.as_ref() == Some(&self.team)
            && self.read.current(&shell.core)
            && self.client.same_configuration(&shell.core.client)
            && self.search.as_ref().is_none_or(|p| p.base_current(shell))
    }
}
pub(super) fn capture(shell: &ShellState, team: &str) -> Option<Owner> {
    if shell.core.active_team.as_deref() != Some(team) {
        return None;
    }
    let transport = shell.core.transport.as_ref()?;
    let session = shell.core.session.as_ref()?.workspaces.get(team)?;
    if session.team_id != team || session.enterprise_id.is_some() {
        return None;
    }
    Some(Owner {
        team: team.into(),
        read: Arc::new(crate::bootstrap::ReadOwner::capture(
            &shell.core,
            transport,
            session,
        )?),
        client: shell.core.client.clone(),
        origin: None,
        search: None,
    })
}
pub(super) fn recapture(shell: &ShellState, original: &Owner) -> Option<Owner> {
    let mut owner = capture(shell, original.team())?;
    owner.origin = original.origin.clone();
    owner.search = original.search.clone();
    Some(owner)
}
/// Created only after the original runner's successful raw response and current
/// ticket check. This borrowed proof cannot be constructed from a Projection,
/// persisted, or reused after its owning profile request leaves the stack.
pub(crate) struct Refreshed<'a> {
    ticket: &'a Ticket,
    value: super_platinum_core::profile_reading::Projection,
}
impl Refreshed<'_> {
    pub(crate) fn details(
        &self,
        shell: &ShellState,
    ) -> Option<(
        &str,
        &Arc<crate::thread_feed::ProfileMember>,
        &super_platinum_core::profile_reading::Projection,
    )> {
        if !self.ticket.current(shell) || self.value.user().id != self.ticket.id {
            return None;
        }
        let owner = self.ticket.member.as_ref()?;
        Some((owner.team(), owner.origin.as_ref()?, &self.value))
    }
}
pub(crate) fn show(shell: &mut ShellState, open: &crate::thread_feed::ProfileOpen) -> bool {
    let Some(projection) = open.user(shell).cloned() else {
        return false;
    };
    let Some((team, read, client, origin)) = open.reading_context(shell) else {
        return false;
    };
    let owner = Owner {
        team,
        read,
        client,
        origin: Some(origin),
        search: None,
    };
    if !owner.current(shell) {
        return false;
    }
    let user = projection.user().clone();
    // Reuse the admitted original details. Explicit Refresh owns new reads;
    // neither opening nor this snapshot inserts a canonical member.
    super::super::discovery::show_profile(shell, user.id.clone());
    let Some(visit) = ProfileVisit::capture(shell).map(Arc::new) else {
        return false;
    };
    shell.profile_search = Some(Snapshot {
        anchor: visit.clone(),
        visit,
        cookie: cookie(shell),
        id: user.id.clone(),
        user: Some(user),
        serial: 1,
        member: Some(owner),
    });
    if let Some(pane) = shell.core.profile_pane.as_mut() {
        pane.loading = false;
    }
    true
}
impl Owner {
    pub(super) fn is_search(&self) -> bool {
        self.search.is_some()
    }
    pub(super) fn search_refresh_started(&self, s: &mut ShellState) {
        if let Some(p) = &self.search {
            p.refresh_started(s);
        }
    }
    pub(super) fn search_data_current(&self, s: &ShellState) -> bool {
        self.search.as_ref().is_none_or(|p| p.data_current(s))
    }
    pub(super) fn search_back(
        &self,
        s: &ShellState,
    ) -> Option<crate::bootstrap::search_message_pages::profiles::Back> {
        let p = self.search.as_ref()?;
        let serial = s.profile_search.as_ref()?.serial;
        p.base_current(s).then(|| p.back(serial))
    }
}
pub(crate) fn show_search(
    s: &mut ShellState,
    open: crate::bootstrap::search_message_pages::profiles::Open,
) -> bool {
    let Some(value) = open.details(s) else {
        return false;
    };
    let Some(mut owner) = capture(s, open.team()) else {
        return false;
    };
    let user = value.user().clone();
    super::super::discovery::show_profile(s, user.id.clone());
    let Some(pane) = open.pane(s) else {
        return false;
    };
    owner.search = Some(pane);
    let Some(visit) = ProfileVisit::capture(s).map(Arc::new) else {
        return false;
    };
    s.profile_search = Some(Snapshot {
        anchor: visit.clone(),
        visit,
        cookie: cookie(s),
        id: user.id.clone(),
        user: Some(user),
        serial: 1,
        member: Some(owner),
    });
    if let Some(p) = s.core.profile_pane.as_mut() {
        p.loading = false;
    }
    true
}
pub(crate) fn back_search(
    s: &mut ShellState,
    token: crate::bootstrap::search_message_pages::profiles::Back,
) -> bool {
    let Some(snapshot) = s.profile_search.as_ref() else {
        return false;
    };
    let Some(p) = snapshot.member.as_ref().and_then(|o| o.search.as_ref()) else {
        return false;
    };
    if !snapshot.anchor.same_owner_pane(s)
        || !snapshot.visit.current(s)
        || !token.current(s, p, snapshot.serial)
        || s.search_open_generation == u64::MAX
        || s.search_input_revision == u64::MAX
        || s.shortcut_generation == u64::MAX
        || s.profile_generation == u64::MAX
    {
        return false;
    }
    s.close_profile();
    s.open_search(false);
    s.overlay == Some(crate::state::Overlay::Search)
}
/// A current original Profile request, never constructible from user data.
pub(crate) struct SearchTicket(Ticket);
impl SearchTicket {
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.0.current(s)
    }
    pub(crate) fn pane(&self) -> &Arc<crate::bootstrap::search_message_pages::profiles::Pane> {
        self.0.member.as_ref().unwrap().search.as_ref().unwrap()
    }
    pub(crate) fn finish(
        &self,
        s: &mut ShellState,
        value: Option<crate::bootstrap::search_message_pages::profiles::Refreshed>,
        transport_failed: bool,
    ) {
        if !self.current(s) {
            if s.profile_search.as_ref().is_some_and(|p| {
                p.serial == self.0.serial
                    && Arc::ptr_eq(&p.visit, &self.0.visit)
                    && Arc::ptr_eq(&p.anchor, &self.0.anchor)
            }) {
                self.0.visit.retire(s);
            }
            return;
        }
        let pair = value.and_then(|v| v.user(s, self.pane()).map(|u| (v, u)));
        if transport_failed {
            if let Some(p) = s.core.profile_pane.as_mut() {
                p.loading = false;
                p.error = Some(FAILED.into());
            }
        } else {
            self.finish_unchecked(s, pair);
        }
    }
    /// Only the caller's already checked short watch encloses this mutation.
    /// No connection/owner/Source-current calls may be added here.
    pub(crate) fn finish_unchecked(
        &self,
        s: &mut ShellState,
        value: Option<(
            crate::bootstrap::search_message_pages::profiles::Refreshed,
            User,
        )>,
    ) {
        let success = value.is_some();
        if let Some(snapshot) = s.profile_search.as_mut() {
            if let Some((data, user)) = value {
                data.bind(self.pane());
                snapshot.user = Some(user);
            } else {
                self.pane().withdraw();
                snapshot.user = None;
            }
        }
        if let Some(p) = s.core.profile_pane.as_mut() {
            p.loading = false;
            p.error = (!success).then(|| FAILED.into());
        }
    }
}
pub(super) async fn run(state: Signal<ShellState>, ticket: Ticket) {
    if ticket.member.as_ref().is_some_and(|o| o.search.is_some()) {
        crate::bootstrap::search_message_pages::profiles::pane_refresh(state, SearchTicket(ticket))
            .await;
        return;
    }
    run_with(state, ticket, |transport, request| async move {
        transport.execute(request).await
    })
    .await;
}
pub(super) async fn run_with<F, Fut>(mut state: Signal<ShellState>, ticket: Ticket, fetch: F)
where
    F: FnOnce(Arc<Transport>, PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    let context = state.try_peek().ok().and_then(|shell| {
        if !ticket.current(&shell) {
            return None;
        }
        let owner = ticket.member.as_ref()?;
        let transport = shell.core.transport.clone()?;
        let session = shell.core.session.as_ref()?.workspaces.get(owner.team())?;
        let request = super_platinum_core::slack::edge::profile_users_info(
            &owner.client,
            session,
            &[ticket.id.clone()],
        )
        .ok()?;
        Some((transport, request))
    });
    let Some((transport, request)) = context else {
        if let Ok(mut shell) = state.try_write() {
            ticket.visit.retire(&mut shell);
        }
        return;
    };
    let result = fetch(transport, request).await;
    let Ok(mut shell) = state.try_write() else {
        return;
    };
    if !ticket.current(&shell) {
        ticket.visit.retire(&mut shell);
        return;
    }
    let team = ticket.member.as_ref().unwrap().team();
    let (projection, transport_failed) = match result {
        Ok(raw) => (
            super_platinum_core::profile_reading::from_reply(&raw, &[ticket.id.clone()], team)
                .ok()
                .and_then(|mut rows| rows.pop()),
            false,
        ),
        Err(_) => (None, true),
    };
    let success = projection.is_some();
    let user = projection.as_ref().map(|p| p.user().clone());
    let new_origin = projection.and_then(|value| {
        crate::thread_feed::publish_profile_refresh(
            &mut shell,
            Refreshed {
                ticket: &ticket,
                value,
            },
        )
    });
    if !success && !transport_failed {
        if let Some(origin) = ticket
            .member
            .as_ref()
            .and_then(|owner| owner.origin.as_ref())
        {
            origin.retire();
        }
    }
    if let Some(snapshot) = shell.profile_search.as_mut() {
        if let (Some(owner), Some(origin)) = (snapshot.member.as_mut(), new_origin) {
            // Future denial must retire the freshly published version, never
            // just the original header version that this refresh superseded.
            owner.origin = Some(origin);
        }
        // An outage retains loaded details; an actual unqualified response
        // clears them instead of reviving old class or namespace facts.
        if success || !transport_failed {
            snapshot.user = user;
        }
    }
    if let Some(pane) = shell.core.profile_pane.as_mut() {
        pane.loading = false;
        pane.error = (!success).then(|| FAILED.into());
    }
}
