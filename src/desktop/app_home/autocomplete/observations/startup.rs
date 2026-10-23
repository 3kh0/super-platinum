use super::*;

/// One scheduling attempt per actual account/source lifecycle. Acquisition and
/// query cancellation never consume or replace this independent reservation.
#[derive(Clone)]
pub(crate) struct Startup {
    pub(super) owner: Arc<Owner>,
    token: Arc<()>,
}
impl Startup {
    pub(crate) fn reserve(
        s: &mut ShellState,
        context: Arc<Context>,
        authority: Authority,
    ) -> Result<Option<Self>, Error> {
        let owner = Arc::new(Owner::capture(s, context, authority)?);
        if Store::capture(s).is_some()
            || s.app_home
                .share_ranking_observations
                .startup
                .as_ref()
                .is_some_and(|v| v.current(s))
        {
            return Ok(None);
        }
        let result = Self {
            owner,
            token: Arc::new(()),
        };
        s.app_home.share_ranking_observations.startup = Some(result.clone());
        Ok(Some(result))
    }
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && s.app_home
                .share_ranking_observations
                .startup
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(&v.token, &self.token))
    }
}
