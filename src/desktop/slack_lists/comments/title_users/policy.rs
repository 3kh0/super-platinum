use super::*;
pub(super) struct Policy {
    context: Context,
    authority: Authority,
    revision: u64,
    pub(super) names: name_policy::Snapshot,
    user: Value,
    enterprise: Option<Value>,
}
impl Policy {
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.context.current(s)
            && self.authority.current(&s.core)
            && self.revision == crate::app_home::autocomplete::preferences_revision(s)
    }
    pub(super) fn snapshot(&self) -> Value {
        serde_json::json!({"user":self.user,"enterprise":self.enterprise,"revision":self.revision})
    }
}
pub(super) async fn acquire(
    state: Signal<ShellState>,
    team: &str,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<Policy, Error> {
    let context = Context::capture(&state.peek(), team)?;
    let authority = Authority::capture(&state.peek().core, team).map_err(|_| unavailable())?;
    let (membership, enterprise) = authority
        .name_policy_membership(&state.peek().core, name_policy::OrdinaryClientV2)
        .map_err(|_| unavailable())?;
    let revision = crate::app_home::autocomplete::preferences_revision(&state.peek());
    let guard = || {
        let s = state.peek();
        current(&s)
            && context.current(&s)
            && authority.current(&s.core)
            && revision == crate::app_home::autocomplete::preferences_revision(&s)
    };
    let user = read(
        state,
        &context,
        name_policy::request::user(&context.client, &context.session),
        &guard,
    )
    .await?;
    let prefs = name_policy::UserPrefs::decode(&user).map_err(|_| unavailable())?;
    let raw = if enterprise.is_some() {
        Some(
            read(
                state,
                &context,
                name_policy::request::enterprise(&context.client, &context.session),
                &guard,
            )
            .await?,
        )
    } else {
        None
    };
    let org = enterprise
        .as_ref()
        .zip(raw.as_ref())
        .map(|(id, raw)| name_policy::EnterprisePrefs::decode(id, raw))
        .transpose()
        .map_err(|_| unavailable())?;
    let names = name_policy::Snapshot::new(name_policy::OrdinaryClientV2, membership, prefs, org)
        .map_err(|_| unavailable())?;
    if !guard() {
        return Err(unavailable());
    }
    Ok(Policy {
        context,
        authority,
        revision,
        names,
        user,
        enterprise: raw,
    })
}
async fn read(
    state: Signal<ShellState>,
    context: &Context,
    request: super_platinum_core::slack::PreparedRequest,
    current: &impl Fn() -> bool,
) -> Result<Value, Error> {
    let work = async {
        if !current() {
            return Err(unavailable());
        }
        if crate::fixture::is_fixture() {
            fixture::completion::title_users::policy_read(state, &request)
                .await
                .unwrap_or_else(|| Err(unavailable()))
        } else {
            context.transport.execute_current(request, current).await
        }
    };
    file_acquisition::run::guarded(work, current).await
}
