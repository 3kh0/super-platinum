//! Management can be entered from About or an exact, visible message action.
use super::*;
use crate::message_buttons::source::Source;

#[derive(Clone)]
pub(super) struct Owner {
    pub(super) ctx: Arc<Context>,
    origin: Origin,
}
#[derive(Clone)]
enum Origin {
    About(super::super::Owner),
    Message { source: Source, cookie: String },
}
impl From<super::super::Owner> for Owner {
    fn from(owner: super::super::Owner) -> Self {
        Self {
            ctx: owner.ctx.clone(),
            origin: Origin::About(owner),
        }
    }
}
impl Owner {
    pub(super) fn message(s: &ShellState, source: Source) -> Option<Self> {
        source.client_message(s).ok()?;
        let ctx = Arc::new(Context::capture(s)?);
        if ctx.session.team_id != source.owner().team_id
            || ctx.session.user_id != source.owner().self_user_id
            || !s.detail_open()
        {
            return None;
        }
        let owner = Self {
            ctx,
            origin: Origin::Message {
                source,
                cookie: s.core.session.as_ref()?.d_cookie.clone(),
            },
        };
        owner.current(s).then_some(owner)
    }
    pub(super) fn direct(&self) -> bool {
        matches!(self.origin, Origin::Message { .. })
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        match &self.origin {
            Origin::About(owner) => owner.current(s),
            Origin::Message { source, cookie } => {
                s.detail_open()
                    && s.core
                        .session
                        .as_ref()
                        .is_some_and(|session| &session.d_cookie == cookie)
                    && source.client_message(s).is_ok()
                    && crate::message_buttons::inputs::user::directory::context_current(
                        s, &self.ctx,
                    )
                    && s.core
                        .session
                        .as_ref()
                        .and_then(|session| session.workspaces.get(&self.ctx.session.team_id))
                        .is_some_and(|w| {
                            w.url == self.ctx.session.url
                                && w.enterprise_id == self.ctx.session.enterprise_id
                        })
            }
        }
    }
}
