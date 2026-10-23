//! Original Save name policy, independent of popup leases and query owners.
use super::*;

pub(crate) struct SaveMemberNames {
    pub(crate) policy: policy::Snapshot,
    pub(crate) gantry: Value,
    pub(crate) enterprise: Option<String>,
    pub(crate) default_workspace: Option<String>,
    context: Arc<crate::scheduled::Context>,
    authority: Authority,
    preferences_revision: u64,
}

impl SaveMemberNames {
    /// `authority` and `proof` come from this Save's fresh own-edit observation.
    /// The injected reader uses the same guarded native request boundary as that
    /// observation; this adapter adds its own authority/context poll admission.
    pub(crate) async fn capture<F, Fut>(
        state: Signal<ShellState>,
        authority: Authority,
        proof: &Value,
        current: impl Fn(&ShellState) -> bool,
        request: F,
    ) -> Result<Self, Error>
    where
        F: Fn(PreparedRequest) -> Fut,
        Fut: Future<Output = Result<Value, Error>>,
    {
        let (context, preferences_revision, membership, enterprise) = {
            let shell = state.peek();
            if !current(&shell) || !authority.current(&shell.core) {
                return Err(unavailable());
            }
            let context =
                Arc::new(crate::scheduled::Context::capture(&shell).ok_or_else(unavailable)?);
            if proof["workspace"].as_str() != Some(&context.session.team_id)
                || proof["user"].as_str() != Some(&context.session.user_id)
            {
                return Err(unavailable());
            }
            let (membership, enterprise) = authority
                .name_policy_membership(&shell.core, policy::OrdinaryClientV2)
                .map_err(|_| unavailable())?;
            (
                context,
                crate::app_home::autocomplete::preferences_revision(&shell),
                membership,
                enterprise,
            )
        };
        let live = |shell: &ShellState| {
            current(shell)
                && context.current(shell)
                && authority.current(&shell.core)
                && preferences_revision
                    == crate::app_home::autocomplete::preferences_revision(shell)
        };
        let ctx = &context;
        let gantry = guarded(
            state,
            &live,
            &request,
            grid::discovery(&ctx.client, &ctx.session),
        )
        .await?;
        let expected: Vec<String> = membership.workspace_ids().map(str::to_owned).collect();
        let default_workspace = facts::validate_gantry(&gantry, &ctx.session, Some(&expected))?;
        let user =
            policy::UserPrefs::decode(&proof["userPreferences"]).map_err(|_| unavailable())?;
        let enterprise_prefs = if let Some(id) = &enterprise {
            let raw = guarded(
                state,
                &live,
                &request,
                policy::request::enterprise(&ctx.client, &ctx.session),
            )
            .await?;
            Some(policy::EnterprisePrefs::decode(id, &raw).map_err(|_| unavailable())?)
        } else {
            None
        };
        let mut policy =
            policy::Snapshot::new(policy::OrdinaryClientV2, membership, user, enterprise_prefs)
                .map_err(|_| unavailable())?;
        observe_gantry_prefs(&mut policy, &gantry)?;
        for id in policy.missing_team_preferences() {
            let prepared = policy::request::team_for(&ctx.client, &ctx.session, &id)
                .map_err(|_| unavailable())?;
            let raw = guarded(state, &live, &request, prepared).await?;
            policy
                .observe_team(policy::TeamPrefs::decode(&id, &raw).map_err(|_| unavailable())?)
                .map_err(|_| unavailable())?;
            if policy.should_display_real_names() == Ok(true) {
                break;
            }
        }
        policy
            .should_display_real_names()
            .map_err(|_| unavailable())?;
        if !live(&state.peek()) {
            return Err(unavailable());
        }
        Ok(Self {
            policy,
            gantry,
            enterprise,
            default_workspace,
            context,
            authority,
            preferences_revision,
        })
    }

    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && self.authority.current(&shell.core)
            && self.preferences_revision
                == crate::app_home::autocomplete::preferences_revision(shell)
    }
}

async fn guarded<F, Fut>(
    state: Signal<ShellState>,
    current: &impl Fn(&ShellState) -> bool,
    reader: &F,
    request: PreparedRequest,
) -> Result<Value, Error>
where
    F: Fn(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    let mut work = std::pin::pin!(reader(request));
    let admitted = std::future::poll_fn(|cx| {
        if !current(&state.peek()) {
            return Poll::Ready(Err(unavailable()));
        }
        work.as_mut().poll(cx)
    });
    let retired = async {
        loop {
            if !current(&state.peek()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    let result = tokio::select! {
        biased;
        _ = retired => Err(unavailable()),
        result = admitted => result,
    };
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    fn state() -> (VirtualDom, Signal<ShellState>) {
        let slot = Rc::new(Cell::new(None));
        let mut dom = VirtualDom::new_with_props(
            |slot: Rc<Cell<Option<Signal<ShellState>>>>| {
                let state =
                    use_signal(|| ShellState::login(crate::media::MediaRegistry::default()));
                slot.set(Some(state));
                rsx! { span {} }
            },
            slot.clone(),
        );
        dom.rebuild_in_place();
        (dom, slot.get().unwrap())
    }

    fn request() -> PreparedRequest {
        let client = super_platinum_core::slack::SlackClient::default();
        let session = super_platinum_core::config::WorkspaceSession {
            team_id: "TSAVE001".into(),
            user_id: "USAVE001".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.invalid".into(),
            token: "synthetic".into(),
        };
        policy::request::user(&client, &session)
    }

    #[tokio::test]
    async fn retired_save_does_not_construct_a_request_future() {
        let (_dom, state) = state();
        let called = Cell::new(false);
        let result = guarded(
            state,
            &|_| false,
            &|_| {
                called.set(true);
                std::future::ready(Ok(Value::Null))
            },
            request(),
        )
        .await;
        assert!(result.is_err());
        assert!(!called.get());
    }

    #[tokio::test]
    async fn request_that_retires_save_cannot_adopt_its_success() {
        let (_dom, state) = state();
        let current = Cell::new(true);
        let result = guarded(
            state,
            &|_| current.get(),
            &|_| async {
                current.set(false);
                Ok(serde_json::json!({"ok":true}))
            },
            request(),
        )
        .await;
        assert!(result.is_err());
    }
}
