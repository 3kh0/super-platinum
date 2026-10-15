//! Actual retained registration and closed native installation. Gates only delay.
use super::*;
use crate::view::composer::protected_enrollment as native;
use controller::Panel;

pub(super) fn start(state: Signal<ShellState>, mut ui: Signal<Panel>) {
    if ui.peek().busy || ui.peek().dialog.is_some() {
        return;
    }
    let Some(b) = binding(&state.peek()) else {
        return;
    };
    ui.write().busy = true;
    ui.write().adopting = true;
    *b.result.lock().unwrap() = None;
    let attempt = barrier_id();
    let visit = Visit::new();
    *b.install_visit.lock().unwrap() = Some(visit.clone());
    b.record(json!({"kind":"adopt-click","attempt":attempt}));
    let task = spawn(Box::pin(async move {
        let result=async{
            if let Some(receipt)=b.host.lock().unwrap().installation(&b.account,&key(b.target())){
                return Ok(json!({"kind":"installed","attempt":attempt,"alreadyInstalled":true,"receipt":inventory::installed(&receipt)}));
            }
            flush_inputs(&b).await.map_err(|_|durable::Error::ReviewRequired)?;
            b.reload(state).await.map_err(|_|durable::Error::Stale)?;
            let expected={let s=state.peek();let host=b.host.lock().unwrap();
                host.discovery().entry(&s,&key(b.target())).and_then(|v|match v.inventory(){Inventory::Managed(r)=>Some(r.clone()),_=>None}).ok_or(durable::Error::ReviewRequired)?};
            let capture=native::capture(state,b.target(),expected.local()).await?;
            b.record(json!({"kind":"install-captured","attempt":attempt,"expected":super::super::native_fixture::row_json(&expected),"registry":b.registry_label(&state.peek()).map_err(|_|durable::Error::Stale)?}));
            let retained=b.adoption.lock().unwrap().clone();
            let request=if let Some(request)=retained{request}else{
                // This gate is before worker registration dispatch, while the
                // real detached registration job waits on its normal Owner gate.
                let guard=if b.armed("registering") {Some(b.owner(b.target()).map_err(|_|durable::Error::Stale)?.gate.clone().lock_owned().await)} else{None};
                let request={let s=state.peek();if !b.current(&s){return Err(durable::Error::Stale);}
                    let selected=b.selected.lock().unwrap().clone();let opened=b.opened.get(&key(b.target())).ok_or(durable::Error::Stale)?;
                    b.host.lock().unwrap().begin_adoption(&s,&selected,opened,expected.clone(),Visit::new())?};
                *b.adoption.lock().unwrap()=Some(request.clone());
                b.record(json!({"kind":"registration-enqueued","attempt":attempt,"adoptionId":request.id(),"dispatchHeldByOwnerGate":guard.is_some()}));
                if guard.is_some() {
                    let retention_id=reserved_registration(&b).await?;
                    b.record(json!({"kind":"registration-reserved","adoptionId":request.id(),"retentionId":retention_id}));
                }
                controller::publish(ui);b.hold("registering").await;drop(guard);request
            };
            request.ready().await;
            let registered={let s=state.peek();b.host.lock().unwrap().adoption(&s,&request)?};
            b.record(json!({"kind":"registered","attempt":attempt,"adoptionId":request.id(),"current":registered.current,"editor":registered.editor.retained_id(),"laneId":registered.lane.id(),"expected":super::super::native_fixture::row_json(&registered.expected)}));
            *b.registered.lock().unwrap()=Some(Arc::new(registered));
            let prepared=crate::view::composer::protected_adoption::prepare(state,&b.host,&request,capture,visit.clone()).await?;
            b.record(json!({"kind":"install-prepared","attempt":attempt,"installAttempt":prepared.id(),"adoptionId":request.id()}));
            controller::publish(ui);
            b.hold("prepared").await;
            let locked=prepared.lock().await?;
            b.record(json!({"kind":"install-locked","attempt":attempt,"installAttempt":locked.id(),"adoptionId":request.id()}));
            controller::publish(ui);
            b.hold("locked").await;
            let receipt={let s=state.peek();if !b.current(&s){return Err(durable::Error::Stale);}locked.install(&mut b.host.lock().unwrap(),&s)?};
            // The shared helper drops only its exact temporary native lease.
            // Checker observes actual readOnly restoration independently.
            b.record(json!({"kind":"installed","attempt":attempt,"receipt":inventory::installed(&receipt)}));
            controller::publish(ui);
            b.hold("installed").await;
            b.acknowledge(&receipt)?;
            Ok(json!({"kind":"installed","attempt":attempt,"alreadyInstalled":false,"receipt":inventory::installed(&receipt)}))
        }.await;
        let mut panel = ui.write();
        panel.busy = false;
        panel.adopting = false;
        match result {
            Ok(value) => {
                panel.message = Some(
                    "Recovery is ready for new input. Your draft and selection were kept.".into(),
                );
                b.result(value);
            }
            Err(error) => {
                panel.message = Some(
                    "The draft or its location changed. Check status before trying again.".into(),
                );
                b.result(json!({"kind":"install-refused","attempt":attempt,"error":err(error)}));
            }
        }
    }));
    WAITER.with_borrow_mut(|old| *old = Some(task));
}
pub(super) fn cancel(mut ui: Signal<Panel>) {
    let Some(b) = BINDING.get() else {
        return;
    };
    if let Some(visit) = b.install_visit.lock().unwrap().take() {
        visit.retire();
    }
    if let Some(task) = WAITER.with_borrow_mut(Option::take) {
        task.cancel();
    }
    let installed = b
        .host
        .lock()
        .unwrap()
        .installation(&b.account, &key(b.target()));
    let mut panel = ui.write();
    panel.busy = false;
    panel.adopting = false;
    panel.dialog = None;
    panel.message = Some(
        if installed.is_some() {
            "Recovery was already installed. Check status to see the retained result."
        } else {
            "Stopped waiting. Check status to recover the earlier registration."
        }
        .into(),
    );
    b.record(json!({"kind":"adoption-cancel-click","installed":installed.as_ref().map(inventory::installed)}));
    b.result(json!({"kind":"canceled","installed":installed.as_ref().map(inventory::installed)}));
}
pub(super) fn check(state: Signal<ShellState>, mut ui: Signal<Panel>) {
    if ui.peek().busy {
        return;
    }
    let Some(b) = binding(&state.peek()) else {
        return;
    };
    ui.write().busy = true;
    *b.result.lock().unwrap() = None;
    b.record(json!({"kind":"adoption-check-click"}));
    let task = spawn(Box::pin(async move {
        let result=async{
            let installed={b.host.lock().unwrap().installation(&b.account,&key(b.target()))};
            if let Some(receipt)=installed{
                b.record(json!({"kind":"installation-recovered","receipt":inventory::installed(&receipt)}));
                b.acknowledge(&receipt)?;
                return Ok(json!({"kind":"installed","receipt":inventory::installed(&receipt),"statusOnly":true}));
            }
            let request=b.adoption.lock().unwrap().clone().ok_or(durable::Error::Stale)?;
            request.ready().await;
            let registered={let s=state.peek();b.host.lock().unwrap().adoption(&s,&request)?};
            let value=json!({"kind":"registered","adoptionId":request.id(),"current":registered.current,"editor":registered.editor.retained_id(),"laneId":registered.lane.id(),"expected":super::super::native_fixture::row_json(&registered.expected),"statusOnly":true});
            *b.registered.lock().unwrap()=Some(Arc::new(registered));Ok(value)
        }.await;
        let mut panel = ui.write();
        panel.busy = false;
        match result {
            Ok(value) => {
                panel.message=Some(if value["kind"]=="installed"{"Recovery is installed. Your draft was kept."}else{"The earlier registration is retained. Continue editing only after a fresh review."}.into());
                b.result(value);
            }
            Err(error) => {
                panel.message =
                    Some("The earlier registration needs review. Your input is retained.".into());
                b.result(json!({"kind":"status-refused","error":err(error)}));
            }
        }
    }));
    WAITER.with_borrow_mut(|old| *old = Some(task));
}

// The job first checks the old lane and expected owner, then reserves an Editor
// slot before awaiting Owner.gate. Observe that real reservation while we hold
// the gate; merely spawning the job does not prove those earlier checks ran.
async fn reserved_registration(b: &Binding) -> Result<String, durable::Error> {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let reserved = {
                let editors = b
                    .account
                    .editors
                    .lock()
                    .map_err(|_| durable::Error::Storage)?;
                match editors.len() {
                    0 => None,
                    1 => {
                        let (id, editor) = editors.iter().next().expect("one reserved slot");
                        if editor.is_some() {
                            return Err(durable::Error::Stale);
                        }
                        Some(id.clone())
                    }
                    _ => return Err(durable::Error::Stale),
                }
            };
            if let Some(id) = reserved {
                return Ok(id);
            }
            // No std mutex guard is retained over this bounded read-only wait.
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| durable::Error::Pending)?
}
