//! Pane-only details for an uncached People result. Never canonical user evidence.
use super::profile_navigation::ProfileVisit;
use crate::state::ShellState;
use dioxus::prelude::*;
use std::{future::Future, sync::Arc};
use super_platinum_core::slack::{Error, api, models::User};

const FAILED: &str = "Could not refresh this profile. Please try again.";
const RETIRED: &str = "Refresh this profile to see its details.";

// Credentials remain private and ephemeral behind the existing ProfileVisit.
pub(crate) struct Snapshot {
    anchor: Arc<ProfileVisit>,
    visit: Arc<ProfileVisit>,
    cookie: Option<String>,
    id: String,
    user: Option<User>,
    serial: u64,
}
pub(crate) struct Ticket {
    anchor: Arc<ProfileVisit>,
    visit: Arc<ProfileVisit>,
    cookie: Option<String>,
    id: String,
    serial: u64,
}
#[derive(Clone)]
pub(crate) struct Retry {
    anchor: Arc<ProfileVisit>,
    serial: u64,
}
pub(crate) struct Presentation<'a> {
    pub id: &'a str,
    pub user: Option<&'a User>,
    pub loading: bool,
    pub error: Option<String>,
    pub retry: Option<Retry>,
}
fn cookie(shell: &ShellState) -> Option<String> {
    shell.core.session.as_ref().map(|s| s.d_cookie.clone())
}
fn cookie_current(shell: &ShellState, expected: &Option<String>) -> bool {
    cookie(shell) == *expected
        && match (expected, &shell.core.transport) {
            (Some(cookie), Some(transport)) => transport.d_cookie() == cookie,
            (None, None) => true,
            _ => false,
        }
}
impl Snapshot {
    fn current(&self, shell: &ShellState) -> bool {
        self.anchor.same_owner_pane(shell)
            && self.visit.current(shell)
            && cookie_current(shell, &self.cookie)
    }
    fn ticket(&self) -> Ticket {
        Ticket {
            anchor: self.anchor.clone(),
            visit: self.visit.clone(),
            cookie: self.cookie.clone(),
            id: self.id.clone(),
            serial: self.serial,
        }
    }
}
impl Ticket {
    fn current(&self, shell: &ShellState) -> bool {
        shell.profile_search.as_ref().is_some_and(|s| {
            s.serial == self.serial
                && s.id == self.id
                && Arc::ptr_eq(&s.anchor, &self.anchor)
                && Arc::ptr_eq(&s.visit, &self.visit)
                && self.visit.current(shell)
                && self.anchor.same_owner_pane(shell)
                && cookie_current(shell, &self.cookie)
        })
    }
}

/// A partial search hit supplies only an identifier. Load authoritative details
/// under the new pane visit; never manufacture a User from highlight metadata.
pub(crate) fn show_id(shell: &mut ShellState, id: &str) -> Option<Ticket> {
    if id.is_empty()
        || id.len() > 255
        || !id.bytes().all(|b| b.is_ascii_alphanumeric())
        || shell
            .core
            .workspaces
            .get(shell.core.active_team.as_ref()?)?
            .users
            .contains_key(id)
    {
        return None;
    }
    super::discovery::show_profile(shell, id.to_owned());
    let visit = Arc::new(ProfileVisit::capture(shell)?);
    let snapshot = Snapshot {
        anchor: visit.clone(),
        visit,
        cookie: cookie(shell),
        id: id.into(),
        user: None,
        serial: 1,
    };
    let ticket = snapshot.ticket();
    shell.profile_search = Some(snapshot);
    Some(ticket)
}

pub(crate) fn show(shell: &mut ShellState, hint: User) -> Option<Ticket> {
    let workspace = shell
        .core
        .workspaces
        .get(shell.core.active_team.as_ref()?)?;
    if hint.id.trim().is_empty() || workspace.users.contains_key(&hint.id) {
        return None;
    }
    super::discovery::show_profile(shell, hint.id.clone());
    let visit = Arc::new(ProfileVisit::capture(shell)?);
    let snapshot = Snapshot {
        anchor: visit.clone(),
        visit,
        cookie: cookie(shell),
        id: hint.id.clone(),
        user: Some(hint),
        serial: 1,
    };
    let ticket = snapshot.ticket();
    shell.profile_search = Some(snapshot);
    Some(ticket)
}

pub(crate) fn presentation<'a>(shell: &'a ShellState, id: &str) -> Option<Presentation<'a>> {
    let s = shell.profile_search.as_ref()?;
    if s.id != id
        || !s.anchor.same_owner_pane(shell)
        || shell
            .core
            .active_team
            .as_ref()
            .and_then(|team| shell.core.workspaces.get(team))?
            .users
            .contains_key(id)
    {
        return None;
    }
    let pane = shell
        .core
        .profile_pane
        .as_ref()
        .filter(|pane| pane.user == id)?;
    let current = s.current(shell);
    let loading = current && pane.loading;
    Some(Presentation {
        id: &s.id,
        user: current.then_some(s.user.as_ref()).flatten(),
        loading,
        error: if current {
            pane.error.clone()
        } else {
            Some(RETIRED.into())
        },
        retry: (!loading).then(|| Retry {
            anchor: s.anchor.clone(),
            serial: s.serial,
        }),
    })
}

fn begin_retry(shell: &mut ShellState, token: Retry, explicit: bool) -> Option<Ticket> {
    let old = shell.profile_search.as_ref()?;
    // Validate the immutable original owner before capturing any fresh frame.
    if old.serial != token.serial
        || !Arc::ptr_eq(&old.anchor, &token.anchor)
        || !token.anchor.same_owner_pane(shell)
    {
        return None;
    }
    let current = old.current(shell);
    let pane = shell
        .core
        .profile_pane
        .as_ref()
        .filter(|p| p.user == old.id)?;
    if (!explicit && !current) || (current && pane.loading) {
        return None;
    }
    let retained = if current { old.user.clone() } else { None };
    let visit = Arc::new(ProfileVisit::capture(shell)?);
    let frame_cookie = cookie(shell);
    let s = shell.profile_search.as_mut()?;
    s.visit = visit;
    s.cookie = frame_cookie;
    s.user = retained;
    s.serial = s.serial.wrapping_add(1);
    let ticket = s.ticket();
    if let Some(pane) = shell.core.profile_pane.as_mut() {
        pane.loading = true;
        pane.error = None;
    }
    Some(ticket)
}

pub(crate) fn retry(mut state: Signal<ShellState>, token: Retry) {
    let ticket = state
        .try_write()
        .ok()
        .and_then(|mut shell| begin_retry(&mut shell, token, true));
    if let Some(ticket) = ticket {
        dioxus::core::spawn_forever(run(state, ticket));
    }
}

pub(crate) async fn run(state: Signal<ShellState>, ticket: Ticket) {
    let context = state.try_read().ok().and_then(|shell| {
        if !ticket.current(&shell) {
            return None;
        }
        Some((
            shell.core.transport.clone()?,
            shell.core.client.clone(),
            shell
                .core
                .session
                .as_ref()?
                .workspaces
                .get(shell.core.active_team.as_ref()?)?
                .clone(),
        ))
    });
    if let Some((transport, client, session)) = context {
        let id = ticket.id.clone();
        run_with(state, ticket, || {
            api::fetch_users_info(&transport, &client, &session, vec![id])
        })
        .await;
    } else {
        finish(
            state,
            &ticket,
            Err(Error::Offline("No profile connection".into())),
        );
    }
}
pub(crate) async fn run_with<F, Fut>(state: Signal<ShellState>, ticket: Ticket, lookup: F)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Vec<User>, Error>>,
{
    if !state.try_read().ok().is_some_and(|s| ticket.current(&s)) {
        return;
    }
    let result = lookup().await;
    finish(state, &ticket, result);
}
fn finish(mut state: Signal<ShellState>, ticket: &Ticket, result: Result<Vec<User>, Error>) {
    let Ok(mut shell) = state.try_write() else {
        return;
    };
    if !ticket.current(&shell) {
        return;
    }
    let found = result
        .ok()
        .and_then(|users| users.into_iter().find(|u| u.id == ticket.id));
    let success = found.is_some();
    if let Some(user) = found {
        shell.profile_search.as_mut().unwrap().user = Some(user);
    }
    if let Some(pane) = shell.core.profile_pane.as_mut() {
        pane.loading = false;
        pane.error = (!success).then(|| FAILED.into());
    }
}

/// Discovery/reconnect must not seed a canonical user for this read-only pane.
pub(crate) async fn refresh_if_owned(mut state: Signal<ShellState>, id: &str) -> bool {
    match state.try_read() {
        Ok(shell) if !shell.profile_search.as_ref().is_some_and(|s| s.id == id) => return false,
        Err(_) => return true,
        _ => {}
    }
    let (owned, ticket) = {
        let Ok(mut shell) = state.try_write() else {
            return true;
        };
        let Some(s) = shell.profile_search.as_ref().filter(|s| s.id == id) else {
            return false;
        };
        if !s.anchor.same_owner_pane(&shell) {
            return true;
        }
        let known = shell
            .core
            .active_team
            .as_ref()
            .and_then(|t| shell.core.workspaces.get(t))
            .is_some_and(|w| w.users.contains_key(id));
        if known {
            shell.profile_search = None;
            return false;
        }
        let token = Retry {
            anchor: s.anchor.clone(),
            serial: s.serial,
        };
        (true, begin_retry(&mut shell, token, false))
    };
    if let Some(ticket) = ticket {
        run(state, ticket).await;
    }
    owned
}

fn failed_current(shell: &ShellState) -> Option<Retry> {
    let snapshot = shell.profile_search.as_ref()?;
    if !snapshot.current(shell) {
        return None;
    }
    if shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t))?
        .users
        .contains_key(&snapshot.id)
    {
        return None;
    }
    let pane = shell
        .core
        .profile_pane
        .as_ref()
        .filter(|pane| pane.user == snapshot.id)?;
    if pane.loading || pane.error.is_none() {
        return None;
    }
    Some(Retry {
        anchor: snapshot.anchor.clone(),
        serial: snapshot.serial,
    })
}

/// Retry the failed visible read before a resource surface can return early.
/// The reconnect visit owns admission only; ProfileVisit owns the REST response.
pub(super) fn schedule_after_outage(mut state: Signal<ShellState>, visit: super::reconnect::Visit) {
    let Some(token) = state
        .try_read()
        .ok()
        .and_then(|shell| failed_current(&shell))
    else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let ticket = {
            let Ok(mut shell) = state.try_write() else {
                return;
            };
            if !visit.current(&shell) {
                return;
            }
            begin_retry(&mut shell, token, false)
        };
        if let Some(ticket) = ticket {
            run(state, ticket).await;
        }
    });
}

#[cfg(test)]
mod tests;
