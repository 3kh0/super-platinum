use super::*;
use std::{collections::BTreeSet, future::Future};

pub(super) async fn hydrate(state: Signal<ShellState>, frame: Arc<Frame>) {
    let activation = {
        let Ok(s) = state.try_read() else {
            return;
        };
        let Some(v) = owned(&s, &frame) else {
            return;
        };
        let Some(a) = v.book.active() else {
            return;
        };
        a.clone()
    };
    let owner = frame.clone();
    hydrate_with(state, frame, move |ids| async move {
        let request = super_platinum_core::slack::edge::users_info(
            owner.parameters.client(),
            owner.parameters.session(),
            &ids,
        )
        .map_err(|_| Error::Api("people_profile_metadata_unavailable".into()))?;
        let current = || {
            state.try_read().ok().is_some_and(|s| {
                owned(&s, &owner).is_some_and(|v| v.book.activation_current(&activation))
            })
        };
        if !current() {
            return Err(Error::Api("people_profile_metadata_retired".into()));
        }
        let rate = owner.rate.clone();
        let admission_rate = rate.clone();
        let raw = owner
            .parameters
            .transport()
            .execute_admitted(
                request,
                current,
                move || !admission_rate.cooling(),
                move |result| {
                    if let Ok(s) = state.try_read() {
                        rate.observe(&s, result);
                    }
                },
            )
            .await?
            .ok_or_else(|| Error::Api("people_profile_metadata_retired".into()))?;
        if raw.get("ok") == Some(&serde_json::Value::Bool(false))
            || !raw.get("results").is_some_and(serde_json::Value::is_array)
        {
            return Err(Error::Api("people_profile_metadata_unavailable".into()));
        }
        let decoded: super_platinum_core::slack::models::EdgeResults<User> =
            serde_json::from_value(raw)
                .map_err(|_| Error::Api("people_profile_metadata_unavailable".into()))?;
        Ok(decoded.results)
    })
    .await;
}
pub(super) async fn hydrate_with<F, Fut>(
    mut state: Signal<ShellState>,
    frame: Arc<Frame>,
    lookup: F,
) where
    F: FnOnce(Vec<String>) -> Fut,
    Fut: Future<Output = Result<Vec<User>, Error>>,
{
    let (activation, ids) = {
        let Ok(s) = state.try_read() else {
            return;
        };
        let Some(v) = owned(&s, &frame) else {
            return;
        };
        let Some(a) = v.book.active().filter(|a| v.book.activation_current(a)) else {
            return;
        };
        if v.metadata
            .get(&a.page.number())
            .is_some_and(|m| Arc::ptr_eq(&m.page, &a.page))
        {
            return;
        }
        let ids: BTreeSet<_> = a.page.hits().iter().map(|h| h.id().to_owned()).collect();
        (a.clone(), ids)
    };
    if ids.is_empty() {
        return;
    }
    let result = lookup(ids.iter().cloned().collect()).await;
    let Ok(mut s) = state.try_write() else {
        return;
    };
    let rate_result = result
        .as_ref()
        .map(|_| serde_json::Value::Null)
        .map_err(Clone::clone);
    frame.rate.observe(&s, &rate_result);
    let Some(v) = owned_mut(&mut s, &frame).filter(|v| v.book.activation_current(&activation))
    else {
        return;
    };
    let Ok(users) = result else {
        return;
    };
    let users = users
        .into_iter()
        .filter(|u| ids.contains(&u.id))
        .map(|u| (u.id.clone(), Arc::new(u)))
        .collect();
    v.metadata.insert(
        activation.page.number(),
        Metadata {
            page: activation.page.clone(),
            users,
        },
    );
    // Authoritative but detached metadata never establishes workspace membership.
    while v.metadata.len() > 8 {
        let old = *v
            .metadata
            .keys()
            .find(|n| **n != activation.page.number())
            .unwrap();
        v.metadata.remove(&old);
    }
}

#[derive(Clone)]
pub(crate) struct RowSource {
    frame: Arc<Frame>,
    owner: Identity,
    activation: Activation,
    index: usize,
}
impl RowSource {
    pub(crate) fn capture(s: &ShellState, index: usize) -> Option<Self> {
        let v = scene(s)?;
        let a = v.book.active()?;
        if !v.book.activation_current(a) || a.page.hits().get(index).is_none() {
            return None;
        }
        Some(Self {
            frame: v.frame.clone(),
            owner: Identity::capture(s)?,
            activation: a.clone(),
            index,
        })
    }
    fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && owned(s, &self.frame).is_some_and(|v| v.book.activation_current(&self.activation))
            && self.activation.page.hits().get(self.index).is_some()
    }
    pub(crate) fn id(&self) -> &str {
        self.activation.page.hits()[self.index].id()
    }
    pub(super) fn prepare(&self, s: &mut ShellState) -> Option<Open> {
        if !self.current(s) {
            return None;
        }
        let id = self.id().to_owned();
        let known = s
            .core
            .workspaces
            .get(self.frame.owner.team())?
            .users
            .contains_key(&id);
        s.dismiss_overlay();
        if s.overlay.is_some() {
            return None;
        }
        if known {
            crate::bootstrap::discovery::show_profile(s, id.clone());
            Some(Open::Known {
                id,
                visit: ProfileVisit::capture(s)?,
            })
        } else {
            crate::bootstrap::profile_search::show_id(s, &id).map(Open::ReadOnly)
        }
    }
}
pub(super) enum Open {
    Known { id: String, visit: ProfileVisit },
    ReadOnly(crate::bootstrap::profile_search::Ticket),
}
pub(crate) fn open(mut state: Signal<ShellState>, source: RowSource) {
    let next = state
        .try_write()
        .ok()
        .and_then(|mut s| source.prepare(&mut s));
    if let Some(next) = next {
        dioxus::core::spawn_forever(async move {
            match next {
                Open::Known { id, visit } => {
                    if state.try_read().ok().is_some_and(|s| visit.current(&s)) {
                        crate::bootstrap::discovery::refresh_open_profile(state, id).await;
                    }
                }
                Open::ReadOnly(ticket) => {
                    crate::bootstrap::profile_search::run(state, ticket).await
                }
            }
        });
    }
}
