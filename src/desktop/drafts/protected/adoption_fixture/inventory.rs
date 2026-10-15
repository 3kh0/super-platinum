//! Full typed inventories and exact process identities are observations only.
use super::super::native_fixture::{local_json, row_json};
use super::*;
pub(super) fn label(target: ComposerTarget) -> &'static str {
    if target == ComposerTarget::Thread {
        "thread"
    } else {
        "channel"
    }
}
pub(crate) async fn durable(b: &Binding) -> Result<Value, String> {
    let loaded = b.account.inventory().await.map_err(err)?;
    if loaded.locals().len() + loaded.managed().len() != 2
        || !loaded.uncommitted().is_empty()
        || !loaded.recovery().is_empty()
    {
        return Err("Unexpected adoption inventory".into());
    }
    let mut result = serde_json::Map::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let scope = Scope {
            account_id: ACCOUNT.into(),
            key: key(target),
        };
        let value = account::wait(b.account.client().map_err(err)?.review_local(scope))
            .await
            .map_err(err)?;
        result.insert(label(target).into(), local_json(&value));
    }
    Ok(Value::Object(result))
}
pub(super) fn displayed(shell: &ShellState, b: &Binding) -> Value {
    let host = b.host.lock().unwrap();
    let d = host.discovery();
    let entries = [ComposerTarget::Channel, ComposerTarget::Thread]
        .into_iter()
        .map(|target| {
            let view = d.entry(shell, &key(target)).map(|v| match v.inventory() {
                Inventory::Local(i) => {
                    json!({"kind":"local","inventory":local_json(i),"uncommitted":v.uncommitted()})
                }
                Inventory::Managed(r) => {
                    json!({"kind":"managed","inventory":row_json(r),"uncommitted":v.uncommitted()})
                }
            });
            (label(target), view)
        })
        .collect::<BTreeMap<_, _>>();
    json!({"status":format!("{:?}",d.status(shell)),"entries":entries,"pendingEnrollment":host.pending_enrollment().is_some()})
}
pub(super) fn installed(receipt: &lifecycle::Installed) -> Value {
    let snapshot = receipt.editor().snapshot();
    json!({"id":receipt.id(),"adoptionId":receipt.adoption_id(),"installAttempt":receipt.attempt_id(),"expected":row_json(receipt.expected()),"installedEditorSerial":receipt.editor_serial(),"reboundFrom":receipt.rebound_from(),"scope":receipt.scope(),"laneId":receipt.lane_id(),"laneSerial":receipt.lane_serial(),"editor":receipt.editor().retained_id(),"writerId":snapshot.writer_id,"enrollment":snapshot.enrollment,"serial":snapshot.serial})
}
pub(crate) fn projection(shell: &ShellState, b: &Binding) -> Value {
    let retained=b.account.retained_editors().unwrap().iter().map(|e|{let v=e.snapshot();json!({"retentionId":e.retained_id(),"scope":e.scope(),"writerId":v.writer_id,"enrollment":v.enrollment,"serial":v.serial,"pending":v.pending,"failedInputs":v.failed_inputs,"running":v.running,"parked":v.parked,"error":v.error.map(err)})}).collect::<Vec<_>>();
    let (routes, locals, adoptions) = {
        let host = b.host.lock().unwrap();
        let routes = [ComposerTarget::Channel, ComposerTarget::Thread]
            .into_iter()
            .map(|target| {
                let route = match host.route_snapshot(shell, &key(target)) {
                    Ok(value) => serde_json::to_value(value).unwrap(),
                    Err(error) => json!({"error":err(error)}),
                };
                (label(target), route)
            })
            .collect::<BTreeMap<_, _>>();
        let locals=host.retained_locals(&b.account).iter().map(|o|match host.local(o){Ok(lane)=>{let s=lane.writer().snapshot();json!({"id":o.id(),"laneId":lane.id(),"scope":lane.scope(),"serial":s.serial,"pending":s.pending,"failedInputs":s.failed_inputs,"running":s.running,"error":s.error.map(err),"inventory":s.inventory.as_ref().map(local_json)})},Err(error)=>json!({"id":o.id(),"error":err(error)})}).collect::<Vec<_>>();
        let adoptions = host
            .retained_adoptions(&b.account)
            .iter()
            .map(|a| a.id().to_owned())
            .collect::<Vec<_>>();
        (routes, locals, adoptions)
    };
    let registered=b.registered.lock().unwrap().as_ref().map(|r|json!({"expected":row_json(&r.expected),"editor":r.editor.retained_id(),"laneId":r.lane.id(),"sampledCurrent":r.current}));
    let selection = {
        let s = b.selected.lock().unwrap();
        json!({"id":s.id(),"account":s.account_id(),"epoch":s.epoch(),"viewId":s.view()})
    };
    let book = [ComposerTarget::Channel, ComposerTarget::Thread]
        .into_iter()
        .map(|target| (label(target), shell.core.drafts.items.get(&key(target))))
        .collect::<BTreeMap<_, _>>();
    json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,"viewId":shell.draft_writes.view_id,"team":shell.core.active_team,"channel":shell.core.active_channel,"thread":shell.thread_root,
        "channelGeneration":shell.channel_generation,"threadGeneration":shell.thread_generation,"fieldScopes":{"channel":crate::view::composer::field_scope(shell,ComposerTarget::Channel),"thread":crate::view::composer::field_scope(shell,ComposerTarget::Thread)},
        "sessionPresent":shell.core.session.is_some(),"transportPresent":shell.core.transport.is_some(),"cachePresent":shell.core.cache.is_some(),"isolated":b.current(shell),
        "book":book,"channelComposer":shell.core.composer,"threadComposer":shell.core.thread_composer,"discovery":displayed(shell,b),"registry":b.registry_label(shell).ok(),"selection":selection,"routes":routes,"localLanes":locals,"retainedAdoptions":adoptions,"retained":retained,"registered":registered,
        "routed":b.routed.load(Ordering::Relaxed),"refused":b.refused.load(Ordering::Relaxed),"fixtureActions":b.actions.lock().unwrap().clone(),"nativeAdmission":b.native.lock().unwrap().clone(),"result":b.result.lock().unwrap().clone(),"gate":b.gate.lock().unwrap().clone()})
}
