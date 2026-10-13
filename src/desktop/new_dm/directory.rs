//! Debounced directory lookup scoped to one account and recipient-dialog visit.
#[cfg(test)]
#[path = "directory_runtime_tests.rs"]
mod runtime_tests;

use super::{Person, Picker, Visit};
use crate::{scheduled::Context, state::ShellState};
use dioxus::{core::Task, prelude::*};
use std::sync::Arc;
use super_platinum_core::slack::{Error, Transport, api, models::User};

#[derive(Clone)]
struct Scope {
    epoch: u64,
    team: String,
    transport: Option<Arc<Transport>>,
    visit: Visit,
}
impl Scope {
    fn capture(shell: &ShellState) -> Option<Self> {
        Some(Self {
            epoch: shell.core.account_epoch,
            team: shell.core.active_team.clone()?,
            transport: shell.core.transport.clone(),
            visit: Visit::capture(shell),
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.epoch == shell.core.account_epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && self.visit.current(shell)
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}
struct Request {
    revision: u64,
    scope: Scope,
}
#[derive(Default)]
pub(crate) struct Directory {
    revision: u64,
    query: String,
    scope: Option<Scope>,
    users: Vec<User>,
    pub loading: bool,
    pub error: Option<&'static str>,
}
pub(super) fn normalize(query: &str) -> String {
    query.trim().trim_start_matches('@').to_lowercase()
}
impl Directory {
    fn begin(&mut self, shell: &ShellState, query: String) -> Option<Request> {
        self.revision = self.revision.wrapping_add(1);
        self.query = query;
        self.users.clear();
        self.error = None;
        self.loading = false;
        self.scope = Scope::capture(shell);
        if self.query.is_empty() {
            return None;
        }
        let scope = self.scope.clone()?;
        if !scope.current(shell) {
            return None;
        }
        self.loading = true;
        Some(Request {
            revision: self.revision,
            scope,
        })
    }
    fn current(&self, request: &Request, shell: &ShellState) -> bool {
        self.revision == request.revision && request.scope.current(shell)
    }
    fn complete(
        &mut self,
        request: &Request,
        shell: &ShellState,
        result: Result<Vec<User>, Error>,
    ) {
        if !self.current(request, shell) {
            return;
        }
        self.loading = false;
        if std::env::var_os("SUPER_PLATINUM_DIRECTORY_TRACE").is_some() {
            if let Ok(users) = &result {
                let workspace = shell.core.workspaces.get(&request.scope.team);
                let uncached = users
                    .iter()
                    .filter(|u| !workspace.is_some_and(|w| w.users.contains_key(&u.id)))
                    .count();
                eprintln!(
                    "super-platinum: directory completed request={} received={} uncached={}",
                    request.revision,
                    users.len(),
                    uncached
                );
            } else {
                eprintln!(
                    "super-platinum: directory failed request={}",
                    request.revision
                );
            }
        }
        match result {
            Ok(users) => self.users = users,
            Err(error) => {
                self.error = Some(match error {
                    Error::Offline(_) => {
                        "Connect to Slack to search more people. Local matches are still available."
                    }
                    Error::RateLimited { .. } => {
                        "Slack is limiting directory searches. Wait a moment, then retry."
                    }
                    _ => "Workspace search couldn’t load. You can still choose a local match.",
                })
            }
        }
    }
    pub fn append(
        &self,
        shell: &ShellState,
        query: &str,
        selected: &[Person],
        mut local: Vec<Person>,
    ) -> Vec<Person> {
        if self.query != normalize(query) || !self.scope.as_ref().is_some_and(|s| s.current(shell))
        {
            return local;
        }
        let Some(ws) = shell
            .core
            .active_team
            .as_ref()
            .and_then(|t| shell.core.workspaces.get(t))
        else {
            return local;
        };
        for result in self.users.iter().take(30) {
            // Cached realtime metadata wins over a search response. Search may
            // match an email or fuzzy spelling that the local substring filter
            // cannot reproduce, so retain those remote hits without re-filtering.
            let user = ws.users.get(&result.id).unwrap_or(result);
            if user.id.is_empty()
                || user.deleted
                || result.deleted
                || user.id == ws.self_user_id
                || selected.iter().any(|p| p.id == user.id)
                || local.iter().any(|p| p.id == user.id)
            {
                continue;
            }
            local.push(Person {
                id: user.id.clone(),
                name: super_platinum_core::state::display_name(Some(user), &user.id),
                handle: user.name.clone().unwrap_or_default(),
            });
        }
        local
    }
    pub(crate) fn hydrate(&self, shell: &mut ShellState, id: &str) -> bool {
        if self
            .scope
            .as_ref()
            .is_some_and(|scope| !scope.current(shell))
        {
            return false;
        }
        let Some(team) = shell.core.active_team.clone() else {
            return false;
        };
        let Some(ws) = shell.core.workspaces.get(&team) else {
            return false;
        };
        let user = ws
            .users
            .get(id)
            .cloned()
            .or_else(|| self.selected_user(shell, id));
        let Some(user) = user.filter(|u| !u.deleted && u.id != ws.self_user_id) else {
            return false;
        };
        shell
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .users
            .entry(id.into())
            .or_insert(user);
        true
    }
    fn selected_user(&self, shell: &ShellState, id: &str) -> Option<User> {
        if !self.scope.as_ref()?.current(shell) {
            return None;
        }
        self.users
            .iter()
            .find(|u| u.id == id && !u.deleted)
            .cloned()
    }
}

pub(super) fn choose(
    mut state: Signal<ShellState>,
    mut picker: Signal<Picker>,
    directory: Signal<Directory>,
    person: Person,
) {
    select(
        &mut state.write(),
        &mut picker.write(),
        &directory.peek(),
        person,
    );
}

fn select(
    shell: &mut ShellState,
    picker: &mut Picker,
    directory: &Directory,
    person: Person,
) -> bool {
    if shell.overlay != Some(crate::state::Overlay::NewDm)
        || directory
            .scope
            .as_ref()
            .is_some_and(|scope| !scope.current(shell))
    {
        return false;
    }
    let Some(team) = shell.core.active_team.clone() else {
        return false;
    };
    let Some(ws) = shell.core.workspaces.get(&team) else {
        return false;
    };
    let user = ws
        .users
        .get(&person.id)
        .cloned()
        .or_else(|| directory.selected_user(shell, &person.id));
    let Some(user) = user.filter(|u| !u.deleted && u.id != ws.self_user_id) else {
        picker.error = Some("That person is no longer available. Search again.".into());
        return false;
    };
    if !picker.add(person) {
        return false;
    }
    // Selection is the only directory action that hydrates global data, so
    // arrivals cannot reorder the local suggestions under the keyboard cursor.
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .entry(user.id.clone())
        .or_insert(user);
    true
}

pub(super) fn use_directory(
    state: Signal<ShellState>,
    picker: Signal<Picker>,
) -> (Signal<Directory>, Signal<u64>) {
    let query = use_memo(move || {
        let p = picker.read();
        if p.pending {
            String::new()
        } else {
            normalize(&p.query)
        }
    });
    use_search(state, query)
}

pub(crate) fn use_search(
    state: Signal<ShellState>,
    query: Memo<String>,
) -> (Signal<Directory>, Signal<u64>) {
    let mut directory = use_signal(move || fixture(&state.peek()));
    let retry = use_signal(|| 0u64);
    let mut task = use_signal(|| None::<Task>);
    let request_key = use_memo(move || {
        let shell = state.read();
        (
            query(),
            shell.core.account_epoch,
            shell.core.active_team.clone(),
            shell
                .core
                .transport
                .as_ref()
                .map(|t| Arc::as_ptr(t) as usize),
            shell.palette_generation,
            shell.channel_generation,
            shell.thread_generation,
            shell.main_view,
            shell.overlay,
            shell.forward_target.as_ref().map(|t| t.id.clone()),
        )
    });
    use_effect(move || {
        let key = request_key();
        let _ = retry();
        if let Some(old) = task.take() {
            old.cancel();
        }
        if crate::fixture::is_fixture() {
            return;
        }
        let Some(request) = directory.write().begin(&state.peek(), key.0.clone()) else {
            return;
        };
        // Only the request key and Retry should subscribe this effect. The
        // general Context::get reads the whole shell and would restart a lookup
        // on every media tick or realtime update, clearing completed results.
        let ctx = Context::capture(&state.peek());
        task.set(Some(spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if !directory.peek().current(&request, &state.peek()) {
                return;
            }
            let result = match ctx {
                Some(ctx) if ctx.current(&state.peek()) => {
                    if std::env::var_os("SUPER_PLATINUM_DIRECTORY_TRACE").is_some() {
                        eprintln!(
                            "super-platinum: directory started request={}",
                            request.revision
                        );
                    }
                    api::fetch_users_search(&ctx.transport, &ctx.client, &ctx.session, key.0).await
                }
                _ => Err(Error::Offline("No active directory connection".into())),
            };
            let Ok(mut result_state) = directory.try_write() else {
                return;
            };
            result_state.complete(&request, &state.peek(), result);
        })));
    });
    (directory, retry)
}

fn fixture(shell: &ShellState) -> Directory {
    let mut directory = Directory::default();
    if let Ok(name) = std::env::var("SUPER_PLATINUM_FIXTURE") {
        if name.starts_with("new-dm-directory") || name.starts_with("forward-people") {
            directory.query = if name.ends_with("empty") {
                "zzznomatch"
            } else {
                "alex"
            }
            .into();
            directory.scope = Scope::capture(shell);
            directory.loading = name.ends_with("loading");
            directory.error = name
                .contains("-error")
                .then_some("Workspace search couldn’t load. You can still choose a local match.");
            if name == "new-dm-directory" || name.starts_with("forward-people") {
                directory.users = vec![
                    User {
                        id: "UREMOTE1".into(),
                        name: Some("alex_rivera".into()),
                        real_name: Some("Alex Rivera".into()),
                        ..Default::default()
                    },
                    User {
                        id: "UREMOTE2".into(),
                        name: Some("alexis_park".into()),
                        real_name: Some("Alexis Park".into()),
                        ..Default::default()
                    },
                ];
            }
        }
    }
    directory
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{MainView, Overlay};
    fn shell() -> ShellState {
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "new-dm")
    }
    fn user(id: &str, name: &str) -> User {
        User {
            id: id.into(),
            name: Some(name.into()),
            real_name: Some(name.into()),
            ..Default::default()
        }
    }
    fn person(id: &str) -> Person {
        Person {
            id: id.into(),
            name: "Remote Person".into(),
            handle: "remote".into(),
        }
    }
    #[test]
    fn forward_visits_reject_old_directory_results_after_reopening() {
        let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), "forward");
        let mut d = Directory::default();
        let old = d.begin(&s, "alex".into()).unwrap();
        d.complete(&old, &s, Ok(vec![user("UREMOTE", "Alex")]));
        assert_eq!(d.append(&s, "alex", &[], vec![]).len(), 1);
        s.forward_target.as_mut().unwrap().id = "next-forward".into();
        assert!(d.append(&s, "alex", &[], vec![]).is_empty());
        assert!(!d.hydrate(&mut s, "UREMOTE"));
        let new = d.begin(&s, "alex".into()).unwrap();
        d.complete(&old, &s, Ok(vec![user("UOLD", "Old")]));
        assert!(d.users.is_empty());
        d.complete(&new, &s, Ok(vec![user("UNEW", "Alex")]));
        assert!(d.hydrate(&mut s, "UNEW"));
        s.overlay = Some(Overlay::NewDm);
        assert!(!d.current(&new, &s));
    }
    #[test]
    fn old_success_and_failure_cannot_replace_a_newer_query_even_after_aba() {
        let s = shell();
        let mut d = Directory::default();
        let old = d.begin(&s, "alex".into()).unwrap();
        d.begin(&s, "maya".into());
        let new = d.begin(&s, "alex".into()).unwrap();
        d.complete(&old, &s, Ok(vec![user("UREMOTE", "Old")]));
        assert!(d.users.is_empty());
        assert!(d.loading);
        d.complete(&old, &s, Err(Error::Api("failure".into())));
        assert!(d.error.is_none());
        assert!(d.loading);
        d.complete(&new, &s, Ok(vec![user("UNEW", "New")]));
        assert_eq!(d.users[0].id, "UNEW");
        assert!(!d.loading);
        d.begin(&s, String::new());
        d.complete(&new, &s, Ok(vec![user("ULATE", "Late")]));
        assert!(d.users.is_empty());
        assert!(!d.loading);
    }
    #[test]
    fn account_transport_and_dialog_changes_reject_completion_and_hydration() {
        let mutations: [fn(&mut ShellState); 7] = [
            |s| s.core.account_epoch += 1,
            |s| s.core.active_team = Some("OTHER".into()),
            |s| s.core.transport = Some(Arc::new(Transport::new("fixture").unwrap())),
            |s| {
                s.dismiss_overlay();
                s.overlay = Some(Overlay::NewDm);
            },
            |s| s.channel_generation += 1,
            |s| s.thread_generation += 1,
            |s| s.main_view = MainView::Dms,
        ];
        for mutate in mutations {
            let mut s = shell();
            s.main_view = MainView::Home;
            let mut d = Directory::default();
            let req = d.begin(&s, "alex".into()).unwrap();
            d.complete(&req, &s, Ok(vec![user("UREMOTE", "Alex")]));
            mutate(&mut s);
            assert!(!d.current(&req, &s));
            assert!(d.selected_user(&s, "UREMOTE").is_none());
            assert!(d.append(&s, "alex", &[], vec![]).is_empty());
            assert!(!select(
                &mut s,
                &mut Picker::default(),
                &d,
                person("UREMOTE")
            ));
        }
    }
    #[test]
    fn remote_hits_append_without_reordering_local_cursor_and_respect_cached_deactivation() {
        let mut s = shell();
        let team = s.core.active_team.clone().unwrap();
        let own = s.core.workspaces[&team].self_user_id.clone();
        s.core.workspaces.get_mut(&team).unwrap().users.insert(
            "UDELETED".into(),
            User {
                deleted: true,
                ..user("UDELETED", "Deleted")
            },
        );
        let mut d = Directory::default();
        let req = d.begin(&s, "email@example.invalid".into()).unwrap();
        d.complete(
            &req,
            &s,
            Ok(vec![
                user("ULOCAL", "Duplicate"),
                user("UREMOTE", "Fuzzy result"),
                user("UREMOTE", "Duplicate"),
                user("UDELETED", "Old active record"),
                user(&own, "Self"),
                user("USELECTED", "Selected"),
                user("", "Invalid"),
            ]),
        );
        let rows = d.append(
            &s,
            "email@example.invalid",
            &[person("USELECTED")],
            vec![person("ULOCAL")],
        );
        assert_eq!(
            rows.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            vec!["ULOCAL", "UREMOTE"]
        );
        assert!(d.append(&s, "unrelated", &[], vec![]).is_empty());
    }
    #[test]
    fn only_selection_hydrates_unknown_people_and_never_overwrites_newer_profile_data() {
        let mut s = shell();
        let team = s.core.active_team.clone().unwrap();
        let mut d = Directory::default();
        let req = d.begin(&s, "remote".into()).unwrap();
        d.complete(&req, &s, Ok(vec![user("UREMOTE", "Remote")]));
        assert!(!s.core.workspaces[&team].users.contains_key("UREMOTE"));
        let mut p = Picker::default();
        assert!(select(&mut s, &mut p, &d, person("UREMOTE")));
        assert!(s.core.workspaces[&team].users.contains_key("UREMOTE"));
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .users
            .get_mut("UREMOTE")
            .unwrap()
            .real_name = Some("New realtime name".into());
        p.remove("UREMOTE");
        assert!(select(&mut s, &mut p, &d, person("UREMOTE")));
        assert_eq!(
            s.core.workspaces[&team].users["UREMOTE"]
                .real_name
                .as_deref(),
            Some("New realtime name")
        );
    }
    #[test]
    fn failures_keep_local_matches_available_and_retry_has_a_fresh_generation() {
        let s = shell();
        let mut d = Directory::default();
        let old = d.begin(&s, "alex".into()).unwrap();
        d.complete(
            &old,
            &s,
            Err(Error::Offline("private transport detail".into())),
        );
        assert!(!d.loading);
        assert!(d.error.unwrap().starts_with("Connect to Slack"));
        assert_eq!(d.append(&s, "alex", &[], vec![person("ULOCAL")]).len(), 1);
        let retry = d.begin(&s, "alex".into()).unwrap();
        assert!(d.error.is_none());
        assert!(!d.current(&old, &s));
        d.complete(&retry, &s, Ok(vec![]));
        assert!(!d.loading);
        assert!(d.error.is_none());
    }
}
