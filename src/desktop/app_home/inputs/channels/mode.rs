//! Native configured-workspace constructor, matched to retained cold/warm startup.
//! This is not a server page flag and never promotes an E-shaped ID by itself.
use super::*;
use super_platinum_core::config::WorkspaceSession;
#[derive(Clone)]
pub(super) struct Observation {
    owner: Owner,
    enterprise: bool,
}
fn same_session(a: &WorkspaceSession, b: &WorkspaceSession) -> bool {
    a.team_id == b.team_id
        && a.enterprise_id == b.enterprise_id
        && a.user_id == b.user_id
        && a.name == b.name
        && a.url == b.url
        && a.token == b.token
}
impl Observation {
    pub(super) fn capture(s: &ShellState, owner: &Owner) -> Option<Self> {
        if !owner.current(s) {
            return None;
        }
        let configured = s
            .core
            .session
            .as_ref()?
            .workspaces
            .get(&owner.ctx.session.team_id)?;
        if !same_session(configured, &owner.ctx.session)
            || configured.user_id.is_empty()
            || configured.token.is_empty()
            || configured.url.is_empty()
        {
            return None;
        }
        let enterprise = match &configured.enterprise_id {
            Some(id) if !id.is_empty() => true,
            None if configured.team_id.starts_with('T')
                && configured.team_id.len() > 1
                && configured
                    .team_id
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) =>
            {
                false
            }
            _ => return None,
        };
        Some(Self {
            owner: owner.clone(),
            enterprise,
        })
    }
    pub(super) fn enterprise(&self) -> bool {
        self.enterprise
    }
    pub(super) fn same(&self, other: &Self) -> bool {
        self.enterprise == other.enterprise
            && same_session(&self.owner.ctx.session, &other.owner.ctx.session)
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s) && Self::capture(s, &self.owner).is_some_and(|v| self.same(&v))
    }
    pub(super) fn proof(&self, s: &ShellState) -> Value {
        json!({"current":self.current(s),"enterprise":self.enterprise,"source":"configured-workspace-constructor",
            "workspace":self.owner.ctx.session.team_id,"enterpriseId":self.owner.ctx.session.enterprise_id})
    }
}
pub(crate) fn unavailable(s: &ShellState) -> Option<&'static str> {
    s.app_home.surface.as_ref().and_then(|h|Observation::capture(s,&h.owner)).is_none()
        .then_some("Channel lookup is unavailable until the account's Enterprise mode is known. Your selection is retained.")
}
