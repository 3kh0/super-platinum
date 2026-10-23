//! Optional Feature admission belongs to the original selected channel. Its
//! failure never turns a successful bookmark Add into a failed Add.
use super::*;
use super_platinum_core::channel_workflows::{
    Collections, Previews, Read,
    feature::Add,
    feature_permission::{Permission, Read as PermissionRead},
};
mod run;
pub(super) use run::{prepare, start};

#[derive(Default)]
pub(super) struct State {
    plan: Option<Add>,
    pub(super) checked: bool,
    pub(super) loading: bool,
    pub(super) notice: Option<String>,
    attempted: bool,
    operation: Option<run::Operation>,
}
impl State {
    pub(super) fn visible(&self) -> bool {
        self.plan.is_some()
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"visible":self.visible(),"checked":self.checked,"loading":self.loading,
            "notice":self.notice,"attempted":self.attempted,
            "count":self.plan.as_ref().map(Add::validated_count),
            "operation":self.operation.as_ref().map(run::Operation::snapshot)})
    }
}
pub(super) async fn load(mut state: Signal<ShellState>, q: &Query) {
    // Older fixture cases retain their closed request contract. Feature cases
    // explicitly enable their own closed permission/preview/mutation endpoint.
    if crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_WORKFLOW_FEATURE_CHECK").as_deref() != Ok("1")
    {
        return;
    }
    let choice = {
        let mut s = state.write();
        if !q.current(&s) {
            return;
        }
        let v = attempt_mut(&mut s, q.id)
            .unwrap()
            .confirmation_add
            .as_mut()
            .unwrap();
        if v.feature.attempted || v.loading || v.operation.is_some() {
            return;
        }
        let Some(choice) = v.choice.clone() else {
            return;
        };
        v.feature.attempted = true;
        v.feature.loading = true;
        choice
    };
    let result = acquire(state, q, &choice).await;
    let mut s = state.write();
    if !q.current(&s) || !choice.current(&s) {
        return;
    }
    let v = attempt_mut(&mut s, q.id)
        .unwrap()
        .confirmation_add
        .as_mut()
        .unwrap();
    // Add remains usable while this optional read runs. Once Add begins, late
    // permission replies may not alter its original native checkbox contract.
    if v.operation.is_some() {
        v.feature.loading = false;
        return;
    }
    v.feature.loading = false;
    match result {
        Ok(plan) => v.feature.plan = Some(plan),
        Err(Error::Api(code))
            if matches!(
                code.as_str(),
                "workflow_feature_not_allowed"
                    | "workflow_feature_duplicate"
                    | "workflow_feature_limit"
            ) => {}
        Err(_) => {
            v.feature.notice =
                Some("Featuring could not be verified. Refresh channel choices to retry.".into())
        }
    }
    v.epoch = next();
    drop(s);
    // Rebind this changed epoch without scheduling another acquisition.
    let fresh = Query::capture(&state.peek(), q.original.visit, q.id);
    if let Some(fresh) = fresh {
        native::bind(&fresh).await;
    }
}

pub(super) async fn acquire(
    state: Signal<ShellState>,
    q: &Query,
    choice: &channels::Choice,
) -> Result<Add, Error> {
    if !q.current(&state.peek()) || !choice.current(&state.peek()) {
        return Err(unavailable());
    }
    let permission = PermissionRead::capture(choice.raw(), &q.ctx().session)?;
    let discovery = q
        .read(
            state,
            permission.discovery(&q.ctx().client, &q.ctx().session)?,
        )
        .await?;
    let raw = q
        .read(
            state,
            permission.request(&q.ctx().client, &q.ctx().session, &discovery)?,
        )
        .await?;
    let manage = permission.permission(&raw)?;
    if manage != Permission::Allowed {
        return Err(Error::Api("workflow_feature_not_allowed".into()));
    }
    let collections = Collections::capture(choice.raw(), &json!({"ok":true,"bookmarks":[]}))?;
    let ids = collections.preview_ids();
    let raw = if ids.is_empty() {
        json!({"ok":true,"triggers":[]})
    } else {
        q.read(
            state,
            Read::previews(ids)?.request(&q.ctx().client, &q.ctx().session),
        )
        .await?
    };
    let previews = Previews::decode(ids, raw)?;
    let s = state.peek();
    if !q.current(&s) || !choice.current(&s) {
        return Err(unavailable());
    }
    let trigger = attempt(&s, q.id)
        .and_then(|a| a.followup.as_ref())
        .and_then(|f| f.current_trigger())
        .ok_or_else(unavailable)?;
    choice.feature_plan(&s, trigger, &previews, manage)
}

pub(super) fn toggle(mut state: Signal<ShellState>, visit: u64, id: u64, checked: bool) {
    let Some(q) = Query::capture(&state.peek(), visit, id) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !native::current(&q, Some(&json!({"kind":"feature","value":checked}))).await {
            return;
        }
        let mut s = state.write();
        if !q.current(&s) {
            return;
        }
        let v = attempt_mut(&mut s, id)
            .unwrap()
            .confirmation_add
            .as_mut()
            .unwrap();
        if v.operation.is_some() || v.loading || !v.feature.visible() {
            return;
        }
        v.feature.checked = checked;
        v.epoch = next();
        drop(s);
        let fresh = Query::capture(&state.peek(), visit, id);
        if let Some(fresh) = fresh {
            native::bind(&fresh).await;
        }
    });
}
