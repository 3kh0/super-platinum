//! JSON is evidence only; opaque proofs continue to authorize actual commands.
use super::*;
fn blocked(value: Option<&super_platinum_core::drafts::store::sync::BlockedLocal>) -> Value {
    value
        .map(|v| json!({"token":v.token,"draft":v.draft}))
        .unwrap_or(Value::Null)
}
fn alternates(values: &[super_platinum_core::drafts::store::sync::LocalAlternate]) -> Value {
    json!(
        values
            .iter()
            .map(|v| json!({"writerId":v.writer_id,"revision":v.revision,"draft":v.draft}))
            .collect::<Vec<_>>()
    )
}
fn origins(values: &[durable::VersionOrigin]) -> Value {
    json!(
        values
            .iter()
            .map(|v| json!({"writerId":v.writer_id(),"origin":v.origin()}))
            .collect::<Vec<_>>()
    )
}
fn row(row: &durable::Row) -> Value {
    json!({"scope":row.scope(),"revision":row.revision(),"state":row.state(),
        "lineage":{"enrollment":row.lineage().enrollment(),"verifiedEpoch":row.lineage().verified_epoch(),"basis":format!("{:?}",row.lineage().basis())}})
}
pub(crate) fn row_json(review: &durable::Review) -> Value {
    json!({"row":row(review.row()),"local":review.local(),"blocked":blocked(review.blocked()),"alternates":alternates(review.alternates()),"origins":origins(review.origins()),"uncommitted":review.uncommitted()})
}
pub(crate) fn local_json(value: &durable::LocalInventory) -> Value {
    json!({"scope":value.scope(),"clock":value.clock(),"row":value.row().map(row),"local":value.local(),"blocked":blocked(value.blocked()),"alternates":alternates(value.alternates()),"origins":origins(value.origins()),"uncommitted":value.uncommitted()})
}
pub(crate) async fn durable(b: &Binding) -> Result<Value, String> {
    // Same initialized Account; never AfterRestart to inspect a live operation.
    let loaded = b.account.inventory().await.map_err(err)?;
    if loaded.managed().len() != 2
        || !loaded.locals().is_empty()
        || !loaded.uncommitted().is_empty()
    {
        return Err("Unexpected protected inventory scope".into());
    }
    let mut result = serde_json::Map::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let owner = b.owner(target)?;
        let value = owner.local_inventory().await.map_err(err)?;
        let review = owner.review().await.map_err(err)?;
        if value.row().is_none_or(|row| !row.same(review.row()))
            || !same_local(value.local(), review.local())
        {
            return Err("Protected paired inventory changed while reading".into());
        }
        result.insert(label(target).into(), local_json(&value));
    }
    Ok(Value::Object(result))
}
pub(crate) fn projection(s: &ShellState, b: &Binding) -> Value {
    let operations = b.account.retained_operations().map(|ops| ops.into_iter().map(|op| {
        let r = op.record();
        let e = r.evidence.lock().unwrap();
        json!({"id":r.id,"scope":r.scope,"epoch":r.epoch,"serial":r.serial,
            "request":"Send","local":r.local.as_ref(),"book":r.book.as_deref(),"phase":format!("{:?}",e.phase),
            "status":format!("{:?}",e.status),"native":e.native.as_deref(),
            "review":e.review.as_ref().map(|v|row_json(v)),
            "consumed":e.consumed.as_ref().map(|v|json!({"before":row_json(&v.before),"after":row_json(&v.after),"serial":v.serial,"nativeClear":format!("{:?}",v.native_clear)})),
            "preservation":e.preservation.map(|v|v.map_err(err)),
            "httpPresent":e.attempt.is_some(),"receiptPresent":e.receipt.is_some(),"live":op.current()})
    }).collect::<Vec<_>>());
    let editors=[ComposerTarget::Channel,ComposerTarget::Thread].into_iter().map(|target|{
        let value=b.editor(target).ok().map(|editor|{
            let snapshot=editor.snapshot();
            json!({"retentionId":editor.retained_id(),"writerId":snapshot.writer_id,"enrollment":snapshot.enrollment,"serial":snapshot.serial,
                "review":snapshot.view.inventory().managed().ok().map(row_json),"uncommitted":snapshot.view.uncommitted(),"pending":snapshot.pending,"running":snapshot.running,"parked":snapshot.parked,"error":snapshot.error.map(|v|format!("{v:?}")),"failedInputs":snapshot.failed_inputs})
        });(label(target),value)
    }).collect::<BTreeMap<_,_>>();
    let mut locals = serde_json::Map::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        locals.insert(
            label(target).into(),
            json!(s.core.drafts.items.get(&key(target))),
        );
    }
    json!({"account":s.core.active_account,"epoch":s.core.account_epoch,"team":s.core.active_team,"channel":s.core.active_channel,"thread":s.thread_root,"viewId":s.draft_writes.view_id,
        "channelGeneration":s.channel_generation,"threadGeneration":s.thread_generation,
        "fieldScopes":{"channel":crate::view::composer::field_scope(s,ComposerTarget::Channel),"thread":crate::view::composer::field_scope(s,ComposerTarget::Thread)},
        "sessionPresent":s.core.session.is_some(),"transportPresent":s.core.transport.is_some(),"cachePresent":s.core.cache.is_some(),"isolated":b.isolated_shell(s),
        "channelComposer":s.core.composer,"threadComposer":s.core.thread_composer,"book":locals,"editors":editors,
        "routed":b.routed.load(Ordering::Relaxed),"refused":b.refused.load(Ordering::Relaxed),"inflight":b.inflight.load(Ordering::Acquire),
        "fixtureActions":b.actions.lock().unwrap().clone(),"nativeAdmission":b.native.lock().unwrap().clone(),"result":b.result.lock().unwrap().clone(),"operations":operations.map_err(err)})
}
fn label(target: ComposerTarget) -> &'static str {
    if target == ComposerTarget::Thread {
        "thread"
    } else {
        "channel"
    }
}
