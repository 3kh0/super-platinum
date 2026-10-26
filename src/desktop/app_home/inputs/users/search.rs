//! Member search uses the existing shared request, decode, policy and cooldown.
use super::*;
use super_platinum_core::slack::models::User;

fn cooling(s: &ShellState, t: &Ticket) -> bool {
    directory::cooling(s, t.owner.ctx.epoch, &t.owner.ctx.session.team_id)
}
pub(crate) fn search(mut state: Signal<ShellState>, ticket: Ticket, query: String) {
    let request = {
        let mut s = state.write();
        if !open_local(&mut s, ticket.clone()) {
            return;
        }
        let choices = cached(&s, &ticket, &query);
        let wait = cooling(&s, &ticket);
        let p = s.app_home.users.picker.as_mut().unwrap();
        if p.query == query && (p.loading || p.error.is_none()) {
            return;
        }
        p.cancel();
        p.query = query.clone();
        p.query_revision = next();
        p.candidates = choices;
        p.error = wait.then(|| {
            "Slack asked us to wait before searching again. Cached choices are shown.".into()
        });
        p.loading = !query.trim().is_empty() && !wait;
        if !p.loading {
            return;
        }
        let (sender, receiver) = tokio::sync::watch::channel(false);
        p.cancel = Some(sender);
        (p.serial, p.query_revision, receiver)
    };
    spawn(async move {
        let (serial, revision, mut cancellation) = request;
        tokio::select! {_=tokio::time::sleep(Duration::from_millis(250))=>{},_=cancellation.changed()=>return}
        let prepared = {
            let mut s = state.write();
            if !request_current(&s, serial, revision) {
                return;
            }
            if cooling(&s, &ticket) {
                let p = s.app_home.users.picker.as_mut().unwrap();
                p.loading = false;
                p.error = Some(
                    "Slack asked us to wait before searching again. Cached choices are shown."
                        .into(),
                );
                return;
            }
            directory::prepare_users(&ticket.owner.ctx, &query)
        };
        let result = match prepared {
            Ok(prepared) => {
                let fixture = fixture::request(&mut state.write(), &ticket, &prepared);
                let read = async {
                    match fixture {
                        Some(receiver) => receiver
                            .await
                            .unwrap_or(Err(Error::Offline("Home member read unavailable".into()))),
                        None => ticket.owner.ctx.transport.execute(prepared).await,
                    }
                };
                tokio::select! {result=read=>result,_=cancellation.changed()=>return}
            }
            Err(error) => Err(error),
        };
        let mut s = state.write();
        if !directory::owner_current(&s, &ticket.owner.ctx) {
            return;
        }
        if let Err(Error::RateLimited {
            retry_after_secs: Some(seconds),
            ..
        }) = &result
        {
            cooldown(&mut s, &ticket, *seconds);
        }
        complete(
            &mut s,
            serial,
            revision,
            result.and_then(|v| directory::decode_users(&v)),
        );
    });
}
fn cooldown(s: &mut ShellState, t: &Ticket, seconds: u64) {
    let now = Instant::now();
    let deadline = now
        .checked_add(Duration::from_secs(seconds.max(1)))
        .unwrap_or(now + Duration::from_secs(86400));
    directory::remember_cooldown(s, t.owner.ctx.epoch, &t.owner.ctx.session.team_id, deadline);
}
pub(super) fn complete(
    s: &mut ShellState,
    serial: u64,
    revision: u64,
    result: Result<Vec<User>, Error>,
) -> bool {
    if !request_current(s, serial, revision) {
        return false;
    }
    let p = s.app_home.users.picker.as_ref().unwrap();
    let ticket = p.ticket.clone();
    let query = p.query.clone();
    match result {
        Ok(users) => {
            let Some(w) = s.core.workspaces.get(&ticket.owner.ctx.session.team_id) else {
                return false;
            };
            let choices = directory::candidates(
                w,
                users
                    .iter()
                    .map(|u| (u, true))
                    .chain(w.users.values().map(|u| (u, false))),
                &query,
            );
            let p = s.app_home.users.picker.as_mut().unwrap();
            p.agent_unavailable |= users
                .iter()
                .any(|u| u.extra.get("is_agentforce_bot") == Some(&Value::Bool(true)));
            p.candidates = choices;
            p.loading = false;
            p.error = None;
        }
        Err(error) => {
            let seconds = match error {
                Error::RateLimited {
                    retry_after_secs: Some(seconds),
                    ..
                } => seconds.max(1),
                _ => 5,
            };
            cooldown(s, &ticket, seconds);
            let p = s.app_home.users.picker.as_mut().unwrap();
            p.loading = false;
            p.error = Some("Could not search Slack. Cached choices are shown.".into());
        }
    }
    true
}
