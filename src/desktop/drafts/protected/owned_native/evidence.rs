//! Read actual full inventories and retained evidence; never construct authority.
use super::super::native_fixture::{local_json, row_json};
use super::*;
pub(crate) async fn inventory(b: &Run) -> Result<Value, String> {
    let loaded = b
        .account
        .owned
        .as_ref()
        .ok_or("Owned route missing")?
        .inventory()
        .await
        .map_err(err)?;
    if loaded.managed().len() + loaded.locals().len() != 2 || !loaded.uncommitted().is_empty() {
        return Err("Unexpected full Owned inventory".into());
    }
    let mut values = serde_json::Map::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let scope = key(target);
        let value = b
            .account
            .owner(scope.clone())
            .map_err(err)?
            .local_inventory()
            .await
            .map_err(err)?;
        if let Some(review) = loaded.managed().get(&scope) {
            if value.row().is_none_or(|r| !r.same(review.row()))
                || !same_local(value.local(), review.local())
            {
                return Err("Full Loaded and local read differ".into());
            }
        } else if loaded.locals().get(&scope).is_none_or(|v| !v.same(&value)) {
            return Err("Full Loaded local row differs".into());
        }
        values.insert(label(target).into(), local_json(&value));
    }
    Ok(
        json!({"managedCount":loaded.managed().len(),"localCount":loaded.locals().len(),"uncommittedCount":loaded.uncommitted().len(),"scopes":values}),
    )
}
pub(crate) fn snapshot(s: &ShellState, b: &Run) -> Value {
    let editors=[ComposerTarget::Channel,ComposerTarget::Thread].into_iter().map(|target|{
        let value=b.editor(target).ok().map(|e|{let v=e.snapshot();json!({"retentionId":e.retained_id(),"writerId":v.writer_id,"enrollment":v.enrollment,"serial":v.serial,
            "local":v.view.local(),"managed":v.view.inventory().managed().ok().map(row_json),"uncommitted":v.view.uncommitted(),"pending":v.pending,"running":v.running,"parked":v.parked,"error":v.error.map(err),"failedInputs":v.failed_inputs})});(label(target),value)
    }).collect::<BTreeMap<_,_>>();
    let parent = b.parent.lock().unwrap().clone();
    let operation=parent.map(|p|{let r=p.record();let e=r.evidence.lock().unwrap();
        json!({"id":r.id,"scope":r.scope,"epoch":r.epoch,"serial":r.serial,"local":r.local.as_ref(),"book":r.book.as_deref(),"live":p.current(),"phase":format!("{:?}",e.phase),"status":format!("{:?}",e.status),
            "native":e.native.as_deref(),"review":e.review.as_ref().map(|v|row_json(v)),
            "consumed":e.consumed.as_ref().map(|v|json!({"before":row_json(&v.before),"after":row_json(&v.after),"serial":v.serial,"nativeClear":format!("{:?}",v.native_clear)})),
            "operation":e.receipt.as_ref().map(|v|v.operation()),"recorded":e.recorded.as_ref().map(|v|row_json(v)),
            "attempt":e.attempt.as_ref().map(|v|json!({"dispatched":v.dispatched,"raw":v.raw,"outcome":format!("{:?}",v.outcome),"issue":v.issue.map(|e|format!("{e:?}"))})),
            "publication":{"initialInventory":e.publication.initial_inventory,"pendingSend":e.publication.pending_send,"sendHelperInvoked":e.publication.send_helper_invoked,"inventory":e.publication.inventory,"queue":e.publication.queue,"afterReleaseInventory":e.publication.after_release_inventory},
            "releasePresent":e.release.is_some(),"localLeasePresent":e.local_lease.is_some(),"handoff":e.handoff.map(|v|v.map_err(err))})
    });
    let ops = b
        .account
        .owned
        .as_ref()
        .unwrap()
        .operations
        .retained()
        .into_iter()
        .map(|v| json!({"family":v.family,"generation":v.generation}))
        .collect::<Vec<_>>();
    json!({"account":b.account.id(),"epoch":b.epoch,"viewId":b.view_id,"current":b.current(s),"sourceCurrent":b.source.current(s),"bindingCurrent":b.network.current(s),"workerCurrent":b.worker.lock().unwrap().current(s),
        "syntheticSession":seed::permits_socket(s),"cachePresent":s.core.cache.is_some(),"channelComposer":s.core.composer,"threadComposer":s.core.thread_composer,
        "book":{"channel":s.core.drafts.items.get(&key(ComposerTarget::Channel)),"thread":s.core.drafts.items.get(&key(ComposerTarget::Thread))},
        "editors":editors,"operation":operation,"retainedJobs":ops,"routed":b.routed.load(Ordering::Acquire),"refused":b.refused.load(Ordering::Acquire)})
}
pub(super) fn label(t: ComposerTarget) -> &'static str {
    if t == ComposerTarget::Thread {
        "thread"
    } else {
        "channel"
    }
}
fn err(e: durable::Error) -> String {
    format!("{e:?}")
}
