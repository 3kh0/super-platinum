//! hXqI's missing-context owning-team scope. j08h absence is a store result,
//! never an inference from internal workspace membership.
use super::*;
use super_platinum_core::state::posting_metadata::authority::{ExternalTeam, ExternalValidation};

#[derive(Clone)]
pub(super) struct Scope {
    origin: Origin,
    dialog: u64,
    metadata: Option<Metadata>,
    external: Option<ExternalTeam>,
}
impl Scope {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.origin.current(s)
            && s.app_home.workflow.as_ref().is_some_and(|d| {
                d.id == self.dialog
                    && self
                        .metadata
                        .as_ref()
                        .is_none_or(|m| d.metadata.as_ref() == Some(m))
            })
            && self.external.as_ref().is_none_or(|e| e.current(&s.core))
    }
    pub(super) fn only_team(&self) -> Option<&str> {
        self.external
            .as_ref()
            .filter(|e| e.value())
            .and_then(|_| self.metadata.as_ref().map(|m| m.owning_team.as_str()))
    }
    pub(super) fn allows(&self, raw: &Value) -> bool {
        self.only_team().is_none_or(|team| connected(raw, team))
    }
}

pub(super) async fn capture(
    mut state: Signal<ShellState>,
    query: &entity_search::Query,
) -> Result<Scope, Error> {
    let mut scope = Scope {
        origin: query.owner().clone(),
        dialog: query.dialog,
        metadata: None,
        external: None,
    };
    // Only MissingContextChannelSelect installs onlyFromTeamId. Custom
    // conversations and every member picker retain their own source options.
    if query.custom || query.members() {
        return query
            .current(&state.peek())
            .then_some(scope)
            .ok_or_else(unavailable);
    }
    let cached_metadata = {
        let s = state.peek();
        if !query.current(&s) {
            return Err(unavailable());
        }
        s.app_home
            .workflow
            .as_ref()
            .and_then(|d| d.metadata.clone())
    };
    let metadata = if let Some(metadata) = cached_metadata {
        metadata
    } else {
        let request = {
            let s = state.peek();
            if !query.current(&s) {
                return Err(unavailable());
            }
            scope.origin.workflow.metadata_request(
                &scope.origin.owner.ctx.client,
                &scope.origin.owner.ctx.session,
                s.app_home.surface.as_ref().unwrap().view.as_ref().unwrap(),
            )?
        };
        scope
            .origin
            .workflow
            .metadata(query.read(state, request).await?)?
    };
    let authority = entity_posting::external_authority(state, query, &metadata.owning_team).await?;
    let existing = authority
        .external_team(&state.peek().core, &metadata.owning_team)
        .ok();
    let external = if let Some(existing) = existing {
        existing
    } else {
        let (read, request) = {
            let mut s = state.write();
            if !query.current(&s) || !authority.current(&s.core) {
                return Err(unavailable());
            }
            match authority
                .external_validation(&s.core)
                .map_err(|_| unavailable())?
            {
                ExternalValidation::Required => {}
                // Another owner or the source900-second throttle must not be
                // replaced with a second attempt or a timer-granted witness.
                ExternalValidation::Pending
                | ExternalValidation::Throttled(_)
                | ExternalValidation::Ready => return Err(unavailable()),
            }
            let read = authority
                .begin_external_validation(&mut s.core)
                .map_err(|_| unavailable())?;
            let request = match read.request(&s.core) {
                Ok(request) => request,
                Err(_) => {
                    read.failed(&mut s.core);
                    return Err(unavailable());
                }
            };
            (read, request)
        };
        // Query::read retains the real transport/Retry-After classification,
        // including a response for a query which was replaced during await.
        let result = query.read(state, request).await;
        let external = {
            let mut s = state.write();
            if !query.current(&s) || !read.current(&s.core) {
                read.failed(&mut s.core);
                return Err(unavailable());
            }
            let raw = match result {
                Ok(raw) => raw,
                Err(error) => {
                    read.failed(&mut s.core);
                    return Err(error);
                }
            };
            read.apply(&mut s.core, &raw).map_err(|_| unavailable())?;
            authority
                .external_team(&s.core, &metadata.owning_team)
                .map_err(|_| unavailable())?
        };
        crate::bootstrap::persist_workspace_metadata(
            &state,
            &scope.origin.owner.ctx.session.team_id,
        );
        external
    };
    let mut s = state.write();
    if !query.current(&s) || !external.current(&s.core) {
        return Err(unavailable());
    }
    // The metadata itself came from this exact workflow/trigger read. Keep it
    // visible to subsequent auth/scopes pages and bind the retained selection.
    s.app_home.workflow.as_mut().unwrap().metadata = Some(metadata.clone());
    scope.metadata = Some(metadata);
    scope.external = Some(external);
    Ok(scope)
}

fn connected(raw: &Value, team: &str) -> bool {
    // ujN9 uses nullish camelCase precedence, then snake_case, then empty.
    // connected_limited_team_ids contributes to j08h history, NOT this filter.
    raw.get("connectedTeamIds")
        .filter(|v| !v.is_null())
        .or_else(|| raw.get("connected_team_ids"))
        .and_then(Value::as_array)
        .is_some_and(|teams| teams.iter().any(|value| value.as_str() == Some(team)))
}
fn unavailable() -> Error {
    Error::Api("home_workflow_external_scope_unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn connected_team_filter_preserves_camel_precedence_and_nullish_fallback() {
        assert!(connected(&json!({"connected_team_ids":["T2"]}), "T2"));
        assert!(connected(
            &json!({"connectedTeamIds":null,"connected_team_ids":["T2"]}),
            "T2"
        ));
        assert!(!connected(
            &json!({"connectedTeamIds":[],"connected_team_ids":["T2"]}),
            "T2"
        ));
    }
    #[test]
    fn limited_connections_and_internal_membership_do_not_grant_filter() {
        for raw in [
            json!({}),
            json!({"connected_limited_team_ids":["T2"]}),
            json!({"internal_team_ids":["T2"]}),
        ] {
            assert!(!connected(&raw, "T2"));
        }
    }
}
