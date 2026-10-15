//! Shared immutable emoji catalog. Work starts only when a picker or shortcode
//! query requests it. Candidate construction runs outside the renderer thread.
use crate::performance::{emoji_lifecycle as probe, emoji_retirement as retirement};
pub(crate) type CatalogPayload = retirement::Payload<Catalog>;
pub(crate) type PickerPayload = retirement::Payload<Vec<PickerChoice>>;
#[path = "emoji_catalog/admission.rs"]
mod admission;
#[path = "emoji_catalog/snapshot.rs"]
mod snapshot;
use crate::{scheduled::Context, state::ShellState};
use dioxus::prelude::*;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use super_platinum_core::{
    composer_emoji::{
        catalog::{Catalog, CustomRecord},
        standard::Version,
    },
    config::WorkspaceSession,
    slack::Transport,
};

#[derive(Clone)]
struct Owner {
    account: Option<String>,
    epoch: u64,
    team: String,
    user: String,
    transport: Option<Arc<Transport>>,
    session: Option<WorkspaceSession>,
}
impl Owner {
    fn capture(shell: &ShellState) -> Option<Self> {
        let team = shell.core.active_team.clone()?;
        let workspace = shell.core.workspaces.get(&team)?;
        let session = shell
            .core
            .session
            .as_ref()
            .and_then(|session| session.workspaces.get(&team))
            .cloned();
        Some(Self {
            account: shell.core.active_account.clone(),
            epoch: shell.core.account_epoch,
            team,
            user: workspace.self_user_id.clone(),
            transport: shell.core.transport.clone(),
            session,
        })
    }
    fn same(&self, other: &Self) -> bool {
        self.account == other.account
            && self.epoch == other.epoch
            && self.team == other.team
            && self.user == other.user
            && sessions_match(self.session.as_ref(), other.session.as_ref())
            && match (&self.transport, &other.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.account == shell.core.active_account
            && self.epoch == shell.core.account_epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell
                .core
                .workspaces
                .get(&self.team)
                .is_some_and(|w| w.self_user_id == self.user)
            && sessions_match(
                self.session.as_ref(),
                shell
                    .core
                    .session
                    .as_ref()
                    .and_then(|session| session.workspaces.get(&self.team)),
            )
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

fn sessions_match(a: Option<&WorkspaceSession>, b: Option<&WorkspaceSession>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => crate::emoji_preferences::same_session(a, b),
        (None, None) => true,
        _ => false,
    }
}

#[derive(Clone)]
pub(crate) struct PickerChoice {
    pub name: String,
    pub glyph: String,
    pub url: Option<String>,
    pub aliases: String,
}

#[derive(Clone)]
pub(crate) struct Selection {
    pub name: String,
    pub query: String,
    pub loaded: Arc<Loaded>,
}

pub(crate) struct Loaded {
    owner: Owner,
    revision: u64,
    locale: Option<String>,
    pub token: String,
    pub catalog: Arc<CatalogPayload>,
    pub picker: Arc<PickerPayload>,
}
#[cfg(feature = "emoji-retirement-observation")]
impl Drop for Loaded {
    fn drop(&mut self) {
        retirement::emit(
            retirement::event::LOADED_FINAL,
            retirement::payload_id(&*self.catalog),
            0,
            0,
        );
    }
}
impl Loaded {
    pub fn current(&self, shell: &ShellState) -> bool {
        self.owner.current(shell)
            && shell
                .core
                .workspaces
                .get(&self.owner.team)
                .is_some_and(|w| {
                    w.custom_emoji_revision == self.revision && w.locale == self.locale
                })
    }
}

struct Building {
    owner: Owner,
    revision: u64,
    locale: Option<String>,
    ticket: u64,
}
#[derive(Default)]
pub(crate) struct State {
    serial: u64,
    ready: HashMap<String, Arc<Loaded>>,
    building: HashMap<String, Building>,
    attempted: HashMap<String, (Owner, u64, Instant)>,
}
impl State {
    fn retire(&mut self, team: &str, ticket: u64) {
        if self
            .building
            .get(team)
            .is_some_and(|building| building.ticket == ticket)
        {
            self.building.remove(team);
        }
    }
}

pub(crate) fn current(shell: &ShellState) -> Option<Arc<Loaded>> {
    let loaded = shell
        .emoji_catalog
        .ready
        .get(shell.core.active_team.as_ref()?)?;
    loaded.current(shell).then(|| loaded.clone())
}

fn build(
    mut custom: Vec<CustomRecord>,
) -> Result<(Arc<CatalogPayload>, Arc<PickerPayload>), String> {
    // emoji.list and the existing Workspace map have already lost server object
    // order. Sort explicitly rather than exposing random HashMap iteration ties.
    custom.sort_by(|a, b| a.name.cmp(&b.name));
    let catalog = Arc::new(retirement::payload(
        Catalog::build_records(Version::V16, custom)?,
        1,
    ));
    let groups: HashMap<_, _> = catalog
        .search_entries
        .iter()
        .map(|entry| (entry.primary.canonical.as_str(), entry))
        .collect();
    let mut picker: Vec<_> = catalog
        .names
        .iter()
        .filter_map(|name| {
            let resolved = catalog.resolve(name, None)?;
            let aliases = groups
                .get(name.as_str())
                .map(|e| {
                    std::iter::once(name.as_str())
                        .chain(e.aliases.iter().map(|a| a.canonical.as_str()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_else(|| name.clone());
            Some(PickerChoice {
                name: name.clone(),
                glyph: resolved.glyph.unwrap_or_default(),
                url: resolved.image_url,
                aliases: aliases.to_lowercase(),
            })
        })
        .collect();
    const COMMON: &[&str] = &[
        "+1",
        "heart",
        "eyes",
        "joy",
        "tada",
        "fire",
        "rocket",
        "white_check_mark",
    ];
    let rank = |name: &str| {
        COMMON
            .iter()
            .position(|n| *n == name)
            .unwrap_or(COMMON.len())
    };
    picker.sort_by(|a, b| {
        rank(&a.name)
            .cmp(&rank(&b.name))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok((catalog, Arc::new(retirement::payload(picker, 2))))
}

fn building_current(
    shell: &ShellState,
    owner: &Owner,
    revision: u64,
    locale: &Option<String>,
    ticket: u64,
) -> bool {
    owner.current(shell)
        && shell
            .core
            .workspaces
            .get(&owner.team)
            .is_some_and(|w| w.custom_emoji_revision == revision && &w.locale == locale)
        && shell
            .emoji_catalog
            .building
            .get(&owner.team)
            .is_some_and(|b| b.ticket == ticket)
}

fn retire_building(mut state: Signal<ShellState>, team: &str, ticket: u64) {
    state.write().emoji_catalog.retire(team, ticket);
}

/// Returns the current snapshot if ready and schedules at most one worker per
/// exact owner/revision. A renderer effect observes only current token/revision
/// changes to requery; unrelated media paints never rebuild the catalog.
pub(crate) fn ensure(mut state: Signal<ShellState>) -> Option<Arc<Loaded>> {
    if probe::enabled() {
        probe::record("ensure", probe_state(&state.peek()));
    }
    let admission::Prepared::Build {
        owner,
        revision,
        locale,
        custom,
        ticket,
        context,
        refresh,
        ready,
    } = (match admission::prepare(&mut state.write(), snapshot::capture) {
        admission::Prepared::Ready(ready) => return ready,
        pending => pending,
    })
    else {
        unreachable!()
    };
    let local_generation = if retirement::enabled() {
        retirement::generation(owner.epoch, ticket, false)
    } else {
        0
    };
    let remote_generation = if retirement::enabled() {
        retirement::generation(owner.epoch, ticket, true)
    } else {
        0
    };
    dioxus::core::spawn_forever(async move {
        let built = tokio::task::spawn_blocking(move || {
            let _build = retirement::building(local_generation);
            let output = snapshot::build_local(custom, ticket, revision);
            if output.is_ok() {
                retirement::hold(local_generation, revision);
            }
            output
        })
        .await;
        if !building_current(&state.peek(), &owner, revision, &locale, ticket) {
            if probe::enabled() {
                probe::record(
                    "rejected-publication",
                    serde_json::json!({"ticket":ticket,"revision":revision}),
                );
            }
            retire_building(state, &owner.team, ticket);
            let _release = retirement::scope(3, 2, 0);
            retirement::outcome(local_generation, false);
            drop(built);
            return;
        }
        match built {
            Ok(Ok((catalog, picker))) => {
                let publish_at = probe::begin();
                let displaced = state.write().emoji_catalog.ready.insert(
                    owner.team.clone(),
                    Arc::new(Loaded {
                        owner: owner.clone(),
                        revision,
                        locale: locale.clone(),
                        token: format!("{ticket}/local"),
                        catalog,
                        picker,
                    }),
                );
                probe::finish(
                    publish_at,
                    "publish",
                    || serde_json::json!({"ticket":ticket,"revision":revision,"token":format!("{ticket}/local")}),
                );
                if probe::enabled() {
                    let counts = displaced.as_ref().map(probe_counts);
                    let at = probe::begin();
                    drop(displaced);
                    probe::finish(
                        at,
                        "displaced-ready-release",
                        || serde_json::json!({"ticket":ticket,"counts":counts}),
                    );
                } else {
                    let _release = retirement::scope(1, 2, 0);
                    drop(displaced);
                }
                retirement::outcome(local_generation, true);
            }
            _ => {
                retire_building(state, &owner.team, ticket);
                return;
            }
        }
        if refresh {
            let fetched = if let Some(result) =
                retirement::remote_response(owner.epoch, revision, ticket).await
            {
                Some(result)
            } else if let Some(context) = context {
                Some(
                    super_platinum_core::slack::api::fetch_emoji_catalog(
                        &context.transport,
                        &context.client,
                        &context.session,
                    )
                    .await,
                )
            } else {
                None
            };
            if let Some(fetched) = fetched {
                if !building_current(&state.peek(), &owner, revision, &locale, ticket) {
                    retire_building(state, &owner.team, ticket);
                    return;
                }
                if let Ok(emojis) = fetched {
                    let built = tokio::task::spawn_blocking(move || {
                        let _build = retirement::building(remote_generation);
                        let (catalog, picker) =
                            build(emojis.iter().map(CustomRecord::from).collect())?;
                        retirement::hold(remote_generation, revision);
                        Ok::<_, String>((emojis, catalog, picker))
                    })
                    .await;
                    if !building_current(&state.peek(), &owner, revision, &locale, ticket) {
                        retire_building(state, &owner.team, ticket);
                        let _release = retirement::scope(4, 2, 0);
                        retirement::outcome(remote_generation, false);
                        drop(built);
                        return;
                    }
                    if let Ok(Ok((emojis, catalog, picker))) = built {
                        let mut shell = state.write();
                        let workspace = shell.core.workspaces.get_mut(&owner.team).unwrap();
                        workspace.replace_emojis(emojis);
                        let revision = workspace.custom_emoji_revision;
                        shell.emoji_catalog.attempted.insert(
                            owner.team.clone(),
                            (owner.clone(), revision, Instant::now()),
                        );
                        let _release = retirement::scope(2, 1, 0);
                        shell.emoji_catalog.ready.insert(
                            owner.team.clone(),
                            Arc::new(Loaded {
                                owner: owner.clone(),
                                revision,
                                locale: locale.clone(),
                                token: format!("{ticket}/remote"),
                                catalog,
                                picker,
                            }),
                        );
                        retirement::outcome(remote_generation, true);
                    }
                }
            }
        }
        retire_building(state, &owner.team, ticket);
    });
    ready
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retired_workspace_build_does_not_strand_or_clear_replacement() {
        let shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let owner = Owner::capture(&shell).unwrap();
        let mut state = State::default();
        let team = owner.team.clone();
        state.building.insert(
            team.clone(),
            Building {
                owner,
                revision: 0,
                locale: None,
                ticket: 2,
            },
        );
        state.retire(&team, 1);
        assert_eq!(state.building[&team].ticket, 2);
        state.retire(&team, 2);
        assert!(!state.building.contains_key(&team));
    }

    #[test]
    fn catalog_owner_preserves_fixture_none_and_rejects_session_replacement() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.session = None;
        let fixture = Owner::capture(&shell).unwrap();
        assert!(fixture.current(&shell));
        let team = shell.core.active_team.clone().unwrap();
        let session = WorkspaceSession {
            team_id: team.clone(),
            user_id: "U-fixture".into(),
            token: "fixture-only-token".into(),
            url: "https://fixture.slack.com".into(),
            enterprise_id: None,
            name: "Fixture".into(),
        };
        shell.core.session = Some(super_platinum_core::config::Session {
            d_cookie: "fixture-only-cookie".into(),
            workspaces: [(team.clone(), session)].into(),
        });
        assert!(!fixture.current(&shell));
        let original = Owner::capture(&shell).unwrap();
        assert!(original.current(&shell));
        shell
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut(&team)
            .unwrap()
            .token
            .push_str("-rotated");
        assert!(!original.current(&shell));
        assert!(!original.same(&Owner::capture(&shell).unwrap()));
    }
}

// Exact fixture diagnostics. No ownership clones in the ledger.
pub(crate) fn probe_state(shell: &ShellState) -> serde_json::Value {
    serde_json::json!({"serial":shell.emoji_catalog.serial,"ready":shell.emoji_catalog.ready.len(),"building":shell.emoji_catalog.building.len(),"attempted":shell.emoji_catalog.attempted.len(),"account":shell.core.active_account,"epoch":shell.core.account_epoch,"team":shell.core.active_team,"revision":shell.core.workspaces.get("T1").map(|w|w.custom_emoji_revision)})
}
pub(crate) fn probe_counts(loaded: &Arc<Loaded>) -> serde_json::Value {
    serde_json::json!({"loaded":Arc::strong_count(loaded),"catalog":Arc::strong_count(&loaded.catalog),"picker":Arc::strong_count(&loaded.picker)})
}

#[cfg(feature = "emoji-retirement-observation")]
pub(crate) fn retirement_observation(shell: &ShellState) -> serde_json::Value {
    let ready = shell.emoji_catalog.ready.get("T1");
    serde_json::json!({
        "serial":shell.emoji_catalog.serial,
        "ready":ready.map(|loaded|serde_json::json!({"token":loaded.token,"generation":retirement::payload_id(&*loaded.catalog),"current":loaded.current(shell),"revision":loaded.revision})),
        "building":shell.emoji_catalog.building.get("T1").map(|b|serde_json::json!({"ticket":b.ticket,"revision":b.revision})),
        "revision":shell.core.workspaces.get("T1").map(|w|w.custom_emoji_revision)
    })
}
