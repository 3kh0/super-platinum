//! Explicit fixture interleavings, using real Owner/Editor/queue operations.
use super::*;
use super_platinum_core::drafts::sync::ConsumeKind;

fn view(view: &View) -> Value {
    json!({"local":view.local,"stored":view.stored.as_ref().map(|s|json!({"revision":s.revision,"state":s.state})),"blocked":view.blocked.as_ref().map(|b|json!({"token":b.token,"local":b.draft})),"uncommitted":view.uncommitted})
}
pub(crate) fn projection(shell: &ShellState, b: &FixtureBinding) -> Value {
    let queue = &shell.core.scheduled;
    let items:BTreeMap<_,_>=queue.items.iter().map(|(id,row)|(id.clone(),json!({"id":row.id,"date_scheduled":row.post_at,"date_created":row.date_created,"client_msg_id":row.client_msg_id,"blocks":row.blocks,"file_ids":row.file_ids,"destinations":row.destinations,"is_deleted":row.is_deleted,"is_sent":row.is_sent,"last_updated_ts":row.last_updated_ts}))).collect();
    let mut managed = serde_json::Map::new();
    let mut editors = serde_json::Map::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let label = if target == ComposerTarget::Channel {
            "channel"
        } else {
            "thread"
        };
        let key = key(target);
        managed.insert(
            label.into(),
            shell
                .managed_drafts
                .entry(ACCOUNT, &key)
                .map(view)
                .unwrap_or(Value::Null),
        );
        let current = shell.draft_writes.writers.get(ACCOUNT, b.epoch, &key);
        editors.insert(label.into(),current.map(|editor|{let s=editor.snapshot();json!({"sameCaptured":b.editors.get(&key).is_some_and(|old|Arc::ptr_eq(old,&editor)),"serial":s.serial,"view":view(&s.view),"pending":s.pending,"running":s.running,"unsaved":s.unsaved})}).unwrap_or(Value::Null));
    }
    json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,"team":shell.core.active_team,"viewId":shell.draft_writes.view_id,
        "queue":{"generation":queue.generation,"matchesCapturedScope":queue.matches_scope(b.epoch,"T1"),"filter":queue.filter.label(),"items":items,"loading":queue.loading,"loaded":queue.loaded,"hasMore":queue.has_more,"limit":queue.limit,"error":queue.error,"busy":queue.busy.iter().cloned().collect::<std::collections::BTreeSet<_>>()},
        "managed":managed,"editors":editors,"composer":shell.core.composer,"threadComposer":shell.core.thread_composer,
        "toast":shell.toast.as_ref().map(|t|t.text.clone())})
}
pub(crate) async fn storage_fault(b: &FixtureBinding, enable: bool) -> Result<(), String> {
    if !b.configured() || b.scenario != "storage-retry-channel" {
        return Err("Storage fault requires exact fixture".into());
    }
    let path = b.path.clone();
    let nonce = b.nonce.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        isolation::validate_directory(&path, &nonce)?;
        let held = path.with_extension("held");
        if enable {
            if !path.is_file() || held.exists() {
                return Err("New storage hold required".into());
            }
            std::fs::rename(&path, &held).map_err(|_| "Hold private SQLite")?;
            std::fs::create_dir(&path).map_err(|_| "Install private storage failure")?;
        } else {
            if !path.is_dir() || !held.is_file() {
                return Err("Exact storage hold missing".into());
            }
            std::fs::remove_dir(&path).map_err(|_| "Remove empty private failure directory")?;
            std::fs::rename(&held, &path).map_err(|_| "Restore private SQLite")?;
        }
        Ok(())
    })
    .await
    .map_err(|_| "Storage fault worker failed")?
}
pub(crate) async fn restore_storage(b: &FixtureBinding) -> Result<(), String> {
    storage_fault(b, false).await
}
pub(crate) async fn interleave(
    mut state: Signal<ShellState>,
    b: &FixtureBinding,
    op: &Operation,
) -> Result<Value, String> {
    if !b.current(&state.peek()) || op != b.operation() {
        return Err("Interleave owner changed".into());
    }
    let before = projection(&state.peek(), b);
    let durable_before = durable(b).await?;
    match b.scenario.as_str() {
        "owner-rotation-channel" => {
            state.write().core.account_epoch += 1;
        }
        "writer-replacement-channel" => {
            let loaded = b
                .owner(&op.scope.key)
                .load()
                .await
                .map_err(|e| format!("Replacement load: {e:?}"))?;
            let mut s = state.write();
            s.draft_writes.remove(ACCOUNT, b.epoch, &op.scope.key);
            s.draft_writes
                .editor(&b.account, b.epoch, op.scope.key.clone(), loaded)
                .map_err(|e| format!("Replacement writer: {e:?}"))?;
            // Settle the real writer metadata before capturing its invariant.
            // Otherwise the normal tick may publish this admitted ticket later.
            crate::drafts::integration::poll(&mut s);
        }
        "queue-reset-channel" => {
            let mut s = state.write();
            s.core.scheduled.ensure_scope(b.epoch, "Tother");
            s.core.scheduled.ensure_scope(b.epoch, "T1");
            s.core.scheduled.filter = super_platinum_core::scheduled::DraftFilter::Drafts;
        }
        "queue-newer-channel" => {
            let mut raw = positive(op);
            raw["draft"]["last_updated_ts"] = json!("13.000001");
            raw["draft"]["blocks"][0]["elements"][0]["elements"][0]["text"] =
                json!("Newer queued schedule");
            let row =
                serde_json::from_value(raw["draft"].clone()).map_err(|_| "Queued fixture row")?;
            state.write().core.scheduled.accept_remote(row);
        }
        "successor-channel" | "reenroll-channel" => {
            let owner = b.owner(&op.scope.key);
            let current = owner
                .load()
                .await
                .map_err(|e| format!("Lineage load: {e:?}"))?;
            let stored = current.stored.as_ref().ok_or("Lineage row missing")?;
            if !matches!(stored.state.workflow(), Workflow::Idle)
                || stored.state.consumption().is_some()
            {
                return Err("Lineage gate must follow positive durable completion".into());
            }
            if b.scenario == "successor-channel" {
                let consumed = owner
                    .consume(stored.revision, ConsumeKind::Schedule { post_at: 501 })
                    .await
                    .map_err(|e| format!("Later intent: {e:?}"))?;
                let loaded = owner
                    .load()
                    .await
                    .map_err(|e| format!("Later writer load: {e:?}"))?;
                if loaded.stored.as_ref().map(|s| s.revision) != Some(consumed.revision) {
                    return Err("Later writer revision changed".into());
                }
                // Keep this writer outside the shell registry: the captured and
                // current UI writers must remain None to test durable lineage.
                let mut writers = checkpoint::Queue::default();
                let editor = writers
                    .editor(&b.account, b.epoch, op.scope.key.clone(), loaded)
                    .map_err(|e| format!("Later writer: {e:?}"))?;
                editor.submit(current.local.clone());
                editor
                    .flush()
                    .await
                    .map_err(|e| format!("Later local preservation: {e:?}"))?;
            } else {
                owner
                    .release(stored.revision)
                    .await
                    .map_err(|e| format!("Release completed owner: {e:?}"))?;
                owner
                    .enroll(current.local.clone(), None)
                    .await
                    .map_err(|e| format!("Reenroll: {e:?}"))?;
            }
            let updated = owner
                .load()
                .await
                .map_err(|e| format!("New lineage load: {e:?}"))?;
            let mut s = state.write();
            let old = s
                .managed_drafts
                .entry(ACCOUNT, &op.scope.key)
                .and_then(|v| v.stored.as_ref())
                .map(|v| v.revision)
                .ok_or("Old shell lineage missing")?;
            s.managed_drafts
                .remove_entry(ACCOUNT, b.epoch, &op.scope.key, old);
            s.managed_drafts
                .apply_view(ACCOUNT, b.epoch, op.scope.key.clone(), updated);
        }
        "unmount-thread" => {
            let mut s = state.write();
            s.main_view = crate::state::MainView::Home;
            s.thread_root = Some(ROOT.into());
            s.sync_composer_drafts();
            s.refresh_thread_from_core();
        }
        _ => return Err("Scenario has no injected interleave".into()),
    }
    Ok(
        json!({"scenario":b.scenario,"before":before,"after":projection(&state.peek(),b),"durableBefore":durable_before,"durableAfter":durable(b).await?}),
    )
}
