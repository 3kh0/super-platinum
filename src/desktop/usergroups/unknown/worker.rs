use super::*;
use std::{cell::RefCell, collections::BTreeSet};
struct Guard {
    state: Signal<ShellState>,
    wake: Wake,
    flight: Option<Flight>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        let mut state = self.state;
        let wake = self.wake.clone();
        let flight = self.flight.take();
        let release = move |s: &mut ShellState| {
            let Some(r) = s.group_unknown.slots.get_mut(&key(&wake.owner)) else {
                return;
            };
            if !Arc::ptr_eq(&r.identity, &wake.identity)
                || r.worker
                    .as_ref()
                    .is_none_or(|w| !Arc::ptr_eq(w, &wake.worker))
            {
                return;
            }
            if let Some(f) = flight {
                r.manager.cancel(&r.scope, &f);
            }
            r.active = false;
            r.worker = None;
        };
        if let Ok(mut s) = state.try_write() {
            release(&mut s);
        } else {
            dioxus::core::spawn_forever(async move {
                release(&mut state.write());
            });
        }
    }
}
struct Attempt {
    wake: Wake,
    flight: Flight,
    source: RefCell<Snapshot>,
    returned: RefCell<BTreeSet<String>>,
    requested: Vec<String>,
    errors: usize,
}
impl Attempt {
    fn current(&self, s: &ShellState) -> bool {
        self.wake.flight_current(s, &self.flight)
            && current(s, &self.wake.owner, &self.source.borrow())
    }
    fn publication_current(&self, s: &ShellState) -> bool {
        self.wake.flight_current(s, &self.flight)
    }
    fn successor(&self, s: &ShellState) {
        if let Ok(next) = snapshot(s, &self.wake.owner) {
            *self.source.borrow_mut() = next;
        }
    }
}
/// Retain every existing metadata row, including unknown/nonexistent flags.
/// Native membership-only rows are not selector metadata and remain requested.
pub(super) fn partition(source: &Snapshot, ids: &[String]) -> (Vec<String>, BTreeSet<String>) {
    let retained = ids
        .iter()
        .filter(|id| {
            source
                .rows()
                .iter()
                .any(|r| r.effective["id"].as_str() == Some(id.as_str()))
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    (
        ids.iter()
            .filter(|id| !retained.contains(*id))
            .cloned()
            .collect(),
        retained,
    )
}
fn begin(s: &mut ShellState, wake: &Wake) -> Result<Option<Attempt>, Error> {
    if !wake.current(s) {
        return Err(unavailable());
    }
    let source = snapshot(s, &wake.owner)?;
    let w = &s.core.workspaces[&wake.owner.session.team_id];
    let r = s
        .group_unknown
        .slots
        .get_mut(&key(&wake.owner))
        .ok_or_else(unavailable)?;
    if !r
        .manager
        .source_current(&r.scope, &w.directory_usergroups, &w.usergroups)
    {
        return Err(unavailable());
    }
    let Some(flight) = r
        .manager
        .begin(&r.scope, clock())
        .map_err(|_| unavailable())?
    else {
        return Ok(None);
    };
    let (requested, retained) = partition(&source, flight.ids());
    r.active = true;
    r.started += 1;
    Ok(Some(Attempt {
        wake: wake.clone(),
        flight,
        source: RefCell::new(source),
        returned: RefCell::new(retained),
        requested,
        errors: 0,
    }))
}
async fn acquire(state: Signal<ShellState>, a: &mut Attempt) -> Result<(), Error> {
    if a.requested.is_empty() {
        return Ok(());
    }
    let (rows, errors) =
        super::super::recovery::acquire_unknown(state, a.wake.owner.clone(), &a.requested, |s| {
            a.current(s)
        })
        .await?;
    if !a.current(&state.peek()) {
        return Err(unavailable());
    }
    a.errors += errors;
    let ids = rows
        .iter()
        .map(|r| {
            r["id"]
                .as_str()
                .filter(|id| a.requested.iter().any(|wanted| wanted.as_str() == *id))
                .map(str::to_owned)
                .ok_or_else(unavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !rows.is_empty() {
        let work = {
            let mut s = state;
            let mut shell = s.write();
            if !a.current(&shell) {
                return Err(unavailable());
            }
            super::super::source::enqueue(
                &mut shell,
                a.wake.owner.clone(),
                super::super::source::Operation::Metadata {
                    origin: super::super::source::Origin::Fetch,
                    rows,
                    native_members: super::super::source::NativeMembers::Preserve,
                },
            )
        };
        let super::super::source::Admission::Tracked(work) = work else {
            return Err(unavailable());
        };
        work.run_with_publication(state, |s| a.publication_current(s), |s| a.successor(s))
            .await?;
        if !a.current(&state.peek()) {
            return Err(unavailable());
        }
    }
    a.returned.borrow_mut().extend(ids);
    Ok(())
}
async fn finish(mut state: Signal<ShellState>, a: &Attempt) -> Result<(), Error> {
    let settlement = {
        let s = state.peek();
        if !a.current(&s) {
            return Err(unavailable());
        }
        let r = &s.group_unknown.slots[&key(&a.wake.owner)];
        let w = &s.core.workspaces[&a.wake.owner.session.team_id];
        r.manager
            .settlement(
                &r.scope,
                &a.flight,
                &w.directory_usergroups,
                &w.usergroups,
                &a.returned.borrow().iter().cloned().collect::<Vec<_>>(),
            )
            .map_err(|_| unavailable())?
    };
    let missing = settlement.missing().ids().to_vec();
    if !missing.is_empty() {
        let work = super::super::source::enqueue_missing(
            &mut state.write(),
            a.wake.owner.clone(),
            settlement.missing(),
        );
        let super::super::source::Admission::Tracked(work) = work else {
            return Err(unavailable());
        };
        work.run_with_publication(state, |s| a.publication_current(s), |s| a.successor(s))
            .await?;
    }
    let mut s = state.write();
    if !a.current(&s) {
        return Err(unavailable());
    }
    let r = s
        .group_unknown
        .slots
        .get_mut(&key(&a.wake.owner))
        .ok_or_else(unavailable)?;
    r.manager
        .published(&r.scope, settlement)
        .map_err(|_| unavailable())?;
    r.active = false;
    r.finished += 1;
    r.last = json!({"requested":a.requested,"retainedOrReturned":a.returned.borrow().iter().collect::<Vec<_>>(),"missing":missing,"errors":a.errors,"attempt":a.flight.attempt()});
    Ok(())
}
pub(super) async fn run(mut state: Signal<ShellState>, wake: Wake) {
    let mut guard = Guard {
        state,
        wake,
        flight: None,
    };
    loop {
        let delay = {
            let s = state.peek();
            if !guard.wake.current(&s) {
                return;
            }
            let r = &s.group_unknown.slots[&key(&guard.wake.owner)];
            if r.manager.pending().is_empty() {
                return;
            }
            r.manager.remaining(clock())
        };
        tokio::time::sleep(delay).await;
        let Ok(Some(mut a)) = begin(&mut state.write(), &guard.wake) else {
            return;
        };
        guard.flight = Some(a.flight.clone());
        loop {
            let result =
                tokio::time::timeout(domain::ATTEMPT_TIMEOUT, acquire(state, &mut a)).await;
            if !a.current(&state.peek()) {
                return;
            }
            match result {
                Ok(Ok(())) => break,
                // Only still-current acquisition failures use original wrapper cleanup.
                Ok(Err(_)) => {
                    a.errors += 1;
                    break;
                }
                Err(_) => {
                    let next = {
                        let mut s = state.write();
                        let r = s.group_unknown.slots.get_mut(&key(&a.wake.owner)).unwrap();
                        r.manager.retry_timeout(&r.scope, &a.flight)
                    };
                    match next {
                        Ok(Some(next)) => {
                            a.flight = next.clone();
                            guard.flight = Some(next);
                        }
                        Ok(None) => {
                            a.errors += 1;
                            break;
                        }
                        Err(_) => return,
                    }
                }
            }
        }
        if finish(state, &a).await.is_err() {
            return;
        }
        guard.flight = None;
    }
}
