//! Protected original source envelopes for the isolated persistence fixture.
use super::*;
use crate::app_home::about::management::activity::fixture::channel::warm;

pub(crate) struct Held {
    owner: Owner,
    payload: source::Payload,
    raw: Value,
}
pub(crate) fn hold(s: &ShellState, raw: Value) -> Result<Held, String> {
    if !warm::enabled(s) {
        return Err("Original warm fixture required".into());
    }
    let ctx = Context::capture(s, "TWORKFLOW").map_err(|e| e.to_string())?;
    let slot = s
        .app_home
        .channel_fetches
        .slots
        .get(&ctx.key())
        .ok_or("Original source absent")?;
    let owner = Owner {
        context: slot.context.clone(),
        token: slot.token.clone(),
    };
    if !owner.current(s) {
        return Err("Original source retired".into());
    }
    let original = source::Owner::capture(
        &ctx.account,
        &ctx.session,
        ctx.transport.d_cookie(),
        ctx.epoch,
    );
    let payload = source::Payload::observed(source::PayloadKind::Extras, original, raw.to_string())
        .map_err(|e| format!("{e:?}"))?;
    Ok(Held {
        owner,
        payload,
        raw,
    })
}
impl Held {
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
    }
    pub(crate) fn deliver(self, mut state: Signal<ShellState>) -> Result<Value, String> {
        if !warm::enabled(&state.peek()) {
            return Err("Warm delivery refused".into());
        }
        let was_current = self.current(&state.peek());
        let result = super::super::dispatch(
            &mut state.write(),
            &self.owner,
            super::super::Prepared {
                payload: self.payload,
                member: None,
            },
        );
        let accepted = result.is_ok();
        super::super::kick(state, self.owner);
        Ok(
            serde_json::json!({"raw":self.raw,"originalCurrent":was_current,"accepted":accepted,
            "error":result.err().map(|e|e.to_string())}),
        )
    }
}
pub(crate) fn foreign(mut state: Signal<ShellState>) -> Result<Value, String> {
    if !warm::enabled(&state.peek()) {
        return Err("Warm foreign probe refused".into());
    }
    let s = state.peek();
    let ctx = Context::capture(&s, "TWORKFLOW").map_err(|e| e.to_string())?;
    let slot = s
        .app_home
        .channel_fetches
        .slots
        .get(&ctx.key())
        .ok_or("Original source absent")?;
    let owner = Owner {
        context: slot.context.clone(),
        token: slot.token.clone(),
    };
    drop(s);
    let raw = serde_json::json!({"id":"TFOREIGN","is_plan_frozen":false,"reconnect_grace_period_start_ts":99});
    let before = capture(&state.peek(), "TWORKFLOW").ok_or("Live PlanInfo absent")?;
    team_upsert(&mut state.write(), &owner, &raw).map_err(|e| format!("{e:?}"))?;
    Ok(serde_json::json!({"raw":raw,"ignored":before.current(&state.peek())}))
}
