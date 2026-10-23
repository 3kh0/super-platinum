//! Exact closed automatic member Save and explicit original known-reader repair.
use super::*;
use crate::directory_member_fetches::{Context, known, search};
use std::sync::OnceLock;
mod reads;
pub(crate) use reads::{discovery, member};
pub(crate) const TEXT: &str = "Known @harbor and @not-found";
pub(crate) const ID: &str = "ULISTMEM1";
pub(crate) const FUTURE: &str = include_str!("plain/member-future.json");
const GENERATION: &str = "plain-member-fixture-original";
pub(crate) fn named() -> bool {
    if super::plain_member_agents::named()
        || super::plain_groups::named()
        || super::plain_mixed::named()
    {
        return true;
    }
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_MEMBER_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-plain-member")
}
#[derive(Default)]
struct Ledger {
    lookups: Vec<Value>,
    requests: Vec<Value>,
    held: Option<(
        usize,
        oneshot::Sender<Result<(Value, Option<Vec<String>>), Error>>,
    )>,
    repair: Option<Value>,
}
fn ledger() -> &'static Mutex<Ledger> {
    static VALUE: OnceLock<Mutex<Ledger>> = OnceLock::new();
    VALUE.get_or_init(Default::default)
}
fn refused() -> Error {
    Error::Offline("Closed plain member fixture refused request".into())
}
fn admit(shell: &ShellState, ctx: &Context) -> Result<(), Error> {
    if !named()
        || !completion::enabled(shell)
        || !ctx.current(shell)
        || ctx.session.team_id != "T1"
        || ctx.session.user_id != completion::SELF
    {
        return Err(refused());
    }
    Ok(())
}
pub(crate) fn observe(value: Value) {
    if !named() {
        return;
    }
    let mut l = ledger().lock().unwrap();
    let serial = l.lookups.len() + 1;
    l.lookups.push(json!({"serial":serial,"observation":value}));
}
pub(crate) fn snapshot() -> Value {
    if !named() {
        return json!({"enabled":false});
    }
    let l = ledger().lock().unwrap();
    json!({"enabled":true,"lookups":l.lookups,"requests":l.requests,"held":l.held.as_ref().map(|v|v.0),"repair":l.repair})
}
pub(crate) fn row(updated: u64) -> Value {
    json!({"id":ID,"team_id":"T1","name":"username-must-not-win","updated":updated,
        "deleted":false,"is_bot":false,"is_restricted":false,"is_ultra_restricted":false,
        "real_name":"Harbor Top","profile":{"display_name":"harbor","real_name":"Harbor Person","email":"harbor@fixture.invalid"},
        "fixture_opaque":{"keep":"original-member","ordered":["source","native"]}})
}
pub(crate) fn seed_boot(boot: &mut Value) {
    if !named() {
        return;
    }
    boot["users"].as_array_mut().unwrap().push(row(101));
    boot["workspaces"] = json!([{"id":"T1"}]);
    super::plain_member_agents::seed_boot(boot);
}
pub(crate) fn gantry(owner: &WorkspaceSession) -> Value {
    json!({"ok":true,"self":{"id":owner.user_id,"name":"fixture-self","team_id":owner.team_id,"is_admin":true,"profile":{}},
        "team":{"id":owner.team_id},"workspaces":[{"id":owner.team_id,"prefs":{"display_real_names":false}}],"default_workspace":owner.team_id})
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
    let ready = search::snapshot(shell, "T1", GENERATION).ok();
    let observed = w
        .directory_members
        .observations(&ctx.account, &ctx.session, ctx.epoch, GENERATION)
        .ok();
    let retained = observed.as_ref().and_then(|o| {
        let ids = o.source_ids().map(str::to_owned).collect::<Vec<_>>();
        o.rows(&w.directory_members, &ids).ok()
    });
    let pairs=|rows:&[super_platinum_core::block_actions::workflow_management::member_hydration::Current]| {
        rows.iter().map(|r|json!({"source":r.source,"native":r.native})).collect::<Vec<_>>()
    };
    let groups = w
        .directory_usergroups
        .snapshot(&ctx.account, &ctx.session, ctx.epoch, &w.usergroups)
        .ok();
    json!({"account":ctx.account,"epoch":ctx.epoch,"team":ctx.session.team_id,"user":ctx.session.user_id,"socket":w.rt_generation,
        "current":ready.as_ref().is_some_and(|v|v.current(shell)),"rows":ready.as_ref().map(|r|pairs(r.rows())),
        "retained":retained.as_ref().map(|r|pairs(r)),"native":w.users.get(ID),
        "pending":w.directory_members.has_pending(&ctx.account,&ctx.session).ok(),
        "required":w.directory_members.required_observations(&w.users),
        "groups":groups.as_ref().map(|g|json!({"rows":g.rows(),"membership":g.membership(),"current":g.current(&w.directory_usergroups,&ctx.account,&ctx.session,ctx.epoch,&w.usergroups)})),
        "knownDriver":known::fixture_status(shell)})
}
pub(crate) struct Repair {
    original: search::Snapshot,
    context: Arc<Context>,
}
impl Repair {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell) && completion::enabled(shell)
    }
    pub(crate) fn original_current(&self, shell: &ShellState) -> bool {
        self.original.current(shell)
    }
}
pub(crate) fn hold(mut state: Signal<ShellState>) -> Result<(Repair, Value), String> {
    let ctx = Arc::new(Context::capture(&state.peek(), "T1").map_err(|e| e.to_string())?);
    admit(&state.peek(), &ctx).map_err(|e| e.to_string())?;
    let original = search::snapshot(&state.peek(), "T1", GENERATION).map_err(|e| e.to_string())?;
    let before = source(&state.peek());
    let raw = json!({"type":"user_change","user":row(102)});
    {
        let mut shell = state.write();
        let generation = shell.core.workspaces["T1"].rt_generation;
        let event = super_platinum_core::slack::events::RtEvent::UserChanged(
            serde_json::from_value(raw["user"].clone())
                .map_err(|_| "Invalid original member event")?,
        );
        crate::realtime::apply_in_view(&mut shell.core, "T1", generation, event.clone(), false);
        crate::directory_member_fetches::automatic::event(&mut shell, "T1", generation, &event);
        shell.refresh_from_core();
    }
    let repair = Repair {
        original,
        context: ctx.clone(),
    };
    let proof = json!({"raw":raw,"before":before,"originalCurrent":repair.original_current(&state.peek()),"source":source(&state.peek())});
    if proof["originalCurrent"] != false || proof["source"]["current"] != false {
        return Err("Original member source did not retire".into());
    }
    dioxus::core::spawn_forever(async move {
        let result = known::recover(state, "T1", &[ID.to_owned()], |s| {
            ctx.current(s) && completion::enabled(s)
        })
        .await;
        ledger().lock().unwrap().repair = Some(match result {
            Ok(()) => json!({"ok":true}),
            Err(e) => json!({"ok":false,"error":e.to_string()}),
        });
    });
    Ok((repair, proof))
}
pub(crate) fn release() -> Result<Value, String> {
    if !named() {
        return Err("Closed member mode required".into());
    }
    let (serial, tx) = ledger()
        .lock()
        .unwrap()
        .held
        .take()
        .ok_or("Missing original held member")?;
    let response = json!({"ok":true,"results":[row(102)],"can_interact":{(ID):true}});
    let retired = tx.is_closed();
    let delivered = tx.send(Ok((response.clone(), None))).is_ok();
    let proof = json!({"serial":serial,"receiverRetired":retired,"delivered":delivered});
    let mut l = ledger().lock().unwrap();
    let r = l
        .requests
        .iter_mut()
        .find(|r| r["serial"] == serial)
        .unwrap();
    r["response"] = response;
    r["disposition"] = proof.clone();
    Ok(proof)
}
