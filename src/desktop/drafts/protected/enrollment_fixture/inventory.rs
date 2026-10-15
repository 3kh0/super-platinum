//! Evidence preserves opaque inventories; JSON never authorizes an operation.
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
        return Err("Unexpected enrollment account inventory".into());
    }
    let mut result = serde_json::Map::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let scope = Scope {
            account_id: ACCOUNT.into(),
            key: key(target),
        };
        // Direct typed Current read: no Owner gate is acquired while the
        // prepared enrollment intentionally holds that gate in an interleave.
        let value = account::wait(b.account.client().map_err(err)?.review_local(scope))
            .await
            .map_err(err)?;
        result.insert(label(target).into(), local_json(&value));
    }
    Ok(Value::Object(result))
}
pub(super) fn displayed(s: &ShellState, b: &Binding) -> Value {
    let d = b.discovery.lock().unwrap();
    let entries=[ComposerTarget::Channel,ComposerTarget::Thread].into_iter().map(|target|{
        let view=d.entry(s,&key(target)).map(|v|match v.inventory(){Inventory::Local(local)=>json!({"kind":"local","inventory":local_json(local),"uncommitted":v.uncommitted()}),Inventory::Managed(review)=>json!({"kind":"managed","inventory":row_json(review),"uncommitted":v.uncommitted()})});
        (label(target),view)
    }).collect::<BTreeMap<_,_>>();
    json!({"status":format!("{:?}",d.status(s)),"entries":entries,"recovery":d.recovery(s).map(|v|v.len()),"pendingEnrollment":d.pending_enrollment().is_some()})
}
pub(crate) fn projection(s: &ShellState, b: &Binding) -> Value {
    let retained=b.account.retained_editors().unwrap().iter().map(|e|{let v=e.snapshot();json!({"retentionId":e.retained_id(),"scope":e.scope(),"writerId":v.writer_id,"enrollment":v.enrollment,"serial":v.serial,"pending":v.pending,"failedInputs":v.failed_inputs,"running":v.running,"parked":v.parked,"error":v.error.map(err)})}).collect::<Vec<_>>();
    let writers=b.writers.iter().map(|(key,w)|{let v=w.snapshot();json!({"key":key,"serial":v.serial,"pending":v.pending,"running":v.running,"error":v.error.map(err),"failedInputs":v.failed_inputs})}).collect::<Vec<_>>();
    let book = [ComposerTarget::Channel, ComposerTarget::Thread]
        .into_iter()
        .map(|target| (label(target), s.core.drafts.items.get(&key(target))))
        .collect::<BTreeMap<_, _>>();
    json!({"account":s.core.active_account,"epoch":s.core.account_epoch,"viewId":s.draft_writes.view_id,"team":s.core.active_team,"channel":s.core.active_channel,"thread":s.thread_root,
        "channelGeneration":s.channel_generation,"threadGeneration":s.thread_generation,"fieldScopes":{"channel":crate::view::composer::field_scope(s,ComposerTarget::Channel),"thread":crate::view::composer::field_scope(s,ComposerTarget::Thread)},
        "sessionPresent":s.core.session.is_some(),"transportPresent":s.core.transport.is_some(),"cachePresent":s.core.cache.is_some(),"isolated":b.current(s),
        "book":book,"channelComposer":s.core.composer,"threadComposer":s.core.thread_composer,"discovery":displayed(s,b),"retained":retained,"localWriters":writers,
        "routed":b.routed.load(Ordering::Relaxed),"refused":b.refused.load(Ordering::Relaxed),"fixtureActions":b.actions.lock().unwrap().clone(),"nativeAdmission":b.native.lock().unwrap().clone(),"result":b.result.lock().unwrap().clone()})
}
