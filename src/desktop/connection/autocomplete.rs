//! Share directory routing from raw native connectivity, never the delayed rail.
//! Native adaptation of LPM7 ONLINE: route = clientOnline, non-Offline health =
//! serviceOnline, disconnected RT = troubleConnecting. Health::Unknown is not
//! positive HTTP evidence; a connected original socket plus observed route is
//! accepted by the existing connection/activity policy. No IO or timer here.
use crate::{directory_member_fetches::Context, state::ShellState};
use serde_json::{Value, json};
use std::time::Instant;
use super_platinum_core::{
    slack::transport::Health,
    workflow_share::search::{AcquisitionRoute, Query},
};

pub(crate) struct Observation {
    owner: Context,
    query: Query,
    route_observed_at: Option<Instant>,
    facts: Facts,
    route: AcquisitionRoute,
}
#[derive(Clone, Copy)]
struct Facts {
    route_observed: bool,
    routable: bool,
    health: Health,
    connected: bool,
}
impl Facts {
    fn capture(shell: &ShellState, owner: &Context) -> Option<Self> {
        Some(Self {
            route_observed: shell.connection.routed_at.is_some(),
            routable: shell.connection.routable,
            health: owner.transport.health(),
            connected: shell
                .core
                .workspaces
                .get(&owner.session.team_id)?
                .rt
                .is_connected(),
        })
    }
    fn online(self) -> bool {
        self.route_observed && self.routable && self.health != Health::Offline && self.connected
    }
}
impl Observation {
    pub(crate) fn capture(shell: &ShellState, team: &str, query: &Query) -> Option<Self> {
        let owner = Context::capture(shell, team).ok()?;
        if !active(shell, &owner) {
            return None;
        }
        let facts = Facts::capture(shell, &owner)?;
        // Explicit ordinary client-v2 consumer policy; no invented WebView
        // data-app observation, preference, experiment or fixture bypass.
        let route = query.acquisition_route(facts.online(), true);
        Some(Self {
            owner,
            query: query.clone(),
            route_observed_at: shell.connection.routed_at,
            facts,
            route,
        })
    }
    pub(crate) fn route(&self) -> AcquisitionRoute {
        self.route
    }
    pub(crate) fn online(&self) -> bool {
        self.facts.online()
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        active(shell, &self.owner)
            && self.owner.current(shell)
            && self
                .route_observed_at
                .is_none_or(|at| shell.connection.routed_at.is_some_and(|now| now >= at))
            && Facts::capture(shell, &self.owner).is_some_and(|facts| {
                facts.online() == self.online()
                    && self.query.acquisition_route(facts.online(), true) == self.route
            })
    }
    pub(crate) fn snapshot(&self) -> Value {
        json!({"route":match self.route {AcquisitionRoute::Local=>"local",AcquisitionRoute::Remote=>"remote"},
            "online":self.online(),"hostPolicy":"OrdinaryClientV2","wire":self.query.wire(),
            "routeObserved":self.facts.route_observed,"routable":self.facts.routable,
            "transportHealth":match self.facts.health {Health::Unknown=>"unknown",Health::Online=>"online",Health::Offline=>"offline"},
            "realtimeConnected":self.facts.connected})
    }
}
fn active(shell: &ShellState, owner: &Context) -> bool {
    shell.signed_in
        && !shell.loading
        && shell.core.active_team.as_ref() == Some(&owner.session.team_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::{slack::realtime::Connection, state::RealtimeStatus};

    fn setup() -> ShellState {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = Some("00000000-0000-4000-8000-000000000082".into());
        crate::app_home::fixture::install(&mut shell);
        shell.signed_in = true;
        shell.loading = false;
        shell.connection.routable = true;
        shell.connection.routed_at = Some(Instant::now());
        let team = shell.core.active_team.clone().unwrap();
        let (tx, _) = tokio::sync::mpsc::channel(1);
        shell.core.workspaces.get_mut(&team).unwrap().rt =
            RealtimeStatus::Connected(Connection::from_sender(tx));
        shell
            .core
            .transport
            .as_ref()
            .unwrap()
            .set_health(Health::Online);
        shell
    }
    fn capture(s: &ShellState, query: &str) -> Observation {
        Observation::capture(
            s,
            s.core.active_team.as_deref().unwrap(),
            &Query::new(query).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn share_autocomplete_native_mapping_and_unobserved_route() {
        for route_observed in [false, true] {
            for routable in [false, true] {
                for connected in [false, true] {
                    for health in [Health::Unknown, Health::Online, Health::Offline] {
                        let mut s = setup();
                        s.connection.routed_at = route_observed.then(Instant::now);
                        s.connection.routable = routable;
                        s.core.transport.as_ref().unwrap().set_health(health);
                        if !connected {
                            let team = s.core.active_team.clone().unwrap();
                            s.core.workspaces.get_mut(&team).unwrap().rt =
                                RealtimeStatus::Disconnected;
                        }
                        let online =
                            route_observed && routable && connected && health != Health::Offline;
                        let o = capture(&s, " ");
                        assert_eq!(o.online(), online);
                        assert_eq!(
                            o.route(),
                            if online {
                                AcquisitionRoute::Remote
                            } else {
                                AcquisitionRoute::Local
                            }
                        );
                        assert_eq!(capture(&s, "@").route(), AcquisitionRoute::Local);
                        assert!(o.current(&s));
                    }
                }
            }
        }
    }
    #[test]
    fn share_autocomplete_positive_resampling_and_health_do_not_retire() {
        let mut s = setup();
        s.core
            .transport
            .as_ref()
            .unwrap()
            .set_health(Health::Unknown);
        let o = capture(&s, "review");
        s.connection.routed_at = Some(Instant::now());
        s.core
            .transport
            .as_ref()
            .unwrap()
            .set_health(Health::Online);
        s.connection.status = crate::state::ConnectionStatus::NoNetwork;
        assert!(o.current(&s), "rail status is not acquisition authority");
        s.connection.routable = false;
        assert!(!o.current(&s));
    }
    #[test]
    fn share_autocomplete_owner_and_raw_connection_retire() {
        for mutation in 0..9 {
            let mut s = setup();
            let o = capture(&s, "");
            let team = s.core.active_team.clone().unwrap();
            match mutation {
                0 => s.core.account_epoch += 1,
                1 => s.core.active_account = Some("other".into()),
                2 => s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
                3 => s
                    .core
                    .transport
                    .as_ref()
                    .unwrap()
                    .set_health(Health::Offline),
                4 => s.connection.routed_at = None,
                5 => s.core.session.as_mut().unwrap().d_cookie.push('x'),
                6 => s.core.workspaces.get_mut(&team).unwrap().rt = RealtimeStatus::Disconnected,
                7 => s.core.active_team = Some("other".into()),
                _ => {
                    s.core.transport = Some(std::sync::Arc::new(
                        super_platinum_core::slack::Transport::new("other-synthetic").unwrap(),
                    ))
                }
            }
            assert!(!o.current(&s), "mutation {mutation}");
        }
    }
}
