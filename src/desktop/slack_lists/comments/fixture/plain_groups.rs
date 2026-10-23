//! Closed original group socket ingress, held before its paired renderer commit.
use super::*;
use crate::directory_member_fetches::Context;
use super_platinum_core::state::directory_usergroups::Snapshot;

pub(crate) const TEXT: &str = "Known @alex-team and @not-found";
pub(crate) const FUTURE: &str = include_str!("plain/group-future.json");
const ID: &str = "SLISTGRP1";
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_GROUP_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-plain-group")
}
pub(crate) fn source(shell: &ShellState) -> Value {
    if !named() {
        return Value::Null;
    }
    let Ok(ctx) = Context::capture(shell, "T1") else {
        return Value::Null;
    };
    let Some(w) = shell.core.workspaces.get("T1") else {
        return Value::Null;
    };
    let saved = w
        .directory_usergroups
        .snapshot(&ctx.account, &ctx.session, ctx.epoch, &w.usergroups)
        .ok();
    json!({"account":ctx.account,"epoch":ctx.epoch,"team":ctx.session.team_id,"user":ctx.session.user_id,
        "socket":w.rt_generation,"pending":w.directory_usergroups.has_pending(),
        "current":saved.as_ref().is_some_and(|s|s.current(&w.directory_usergroups,&ctx.account,&ctx.session,ctx.epoch,&w.usergroups)),
        "rows":saved.as_ref().map(Snapshot::rows),"membership":saved.as_ref().and_then(Snapshot::membership),
        "native":w.usergroups.get(ID)})
}
pub(crate) struct Held {
    context: Arc<Context>,
    original: Snapshot,
    work: crate::usergroups::source::Work,
}
fn original_current(original: &Snapshot, ctx: &Context, shell: &ShellState) -> bool {
    shell.core.workspaces.get("T1").is_some_and(|w| {
        original.current(
            &w.directory_usergroups,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            &w.usergroups,
        )
    })
}
pub(crate) fn hold(mut state: Signal<ShellState>) -> Result<(Held, Value), String> {
    let ctx = Arc::new(Context::capture(&state.peek(), "T1").map_err(|e| e.to_string())?);
    if !named() || !completion::enabled(&state.peek()) || !ctx.current(&state.peek()) {
        return Err("Closed original group fixture required".into());
    }
    let original = {
        let shell = state.peek();
        let w = &shell.core.workspaces["T1"];
        w.directory_usergroups
            .snapshot(&ctx.account, &ctx.session, ctx.epoch, &w.usergroups)
            .map_err(|e| format!("Original group source unavailable: {e:?}"))?
    };
    let before = source(&state.peek());
    // Authored incoming metadata retains the original observed row's opaque data.
    let mut incoming = original
        .rows()
        .iter()
        .find(|r| r.source["id"] == ID)
        .ok_or("Original canonical group absent")?
        .source
        .clone();
    incoming["date_update"] = json!(102);
    let raw = json!({"type":"subteam_updated","subteam":incoming});
    let event = super_platinum_core::slack::realtime::parse_event(&raw.to_string())
        .ok_or("Invalid original group event")?;
    let work = {
        let mut shell = state.write();
        let generation = shell.core.workspaces["T1"].rt_generation;
        crate::usergroups::prepare_event(&mut shell, "T1", generation, &event)
            .ok_or("Original group ingress refused")?
    };
    let proof = json!({"raw":raw,"before":before,"source":source(&state.peek()),
        "originalCurrent":original_current(&original,&ctx,&state.peek()),"contextCurrent":ctx.current(&state.peek())});
    if proof["originalCurrent"] != false
        || proof["source"]["pending"] != true
        || proof["source"]["current"] != false
        || proof["before"]["native"] != proof["source"]["native"]
    {
        return Err("Original group work did not hold the native preimage".into());
    }
    Ok((
        Held {
            context: ctx,
            original,
            work,
        },
        proof,
    ))
}
impl Held {
    pub(crate) async fn release(self, state: Signal<ShellState>) -> Result<Value, String> {
        let Self {
            context,
            original,
            work,
        } = self;
        work.run(state, |s| {
            named() && completion::enabled(s) && context.current(s)
        })
        .await
        .map_err(|e| e.to_string())?;
        let proof = json!({"source":source(&state.peek()),"originalCurrent":original_current(&original,&context,&state.peek()),
            "contextCurrent":context.current(&state.peek()),"pairedCommit":true});
        if proof["source"]["current"] != true
            || proof["source"]["pending"] != false
            || proof["originalCurrent"] != false
            || proof["contextCurrent"] != true
        {
            return Err("Group commit lost original context or resurrected old source".into());
        }
        Ok(proof)
    }
}
