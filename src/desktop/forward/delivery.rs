//! Async delivery and recovery keep their original account and request identity.
use super::*;
use super_platinum_core::forwarding::{
    Deliveries, Status,
    store::{Begin, Receipt},
};

pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn context(state: &Signal<ShellState>, target: &ForwardTarget) -> Result<Context, String> {
    let ctx = Context::get(state, target)
        .ok_or("Connect to Slack to forward this message. Your note is still here.")?;
    if ctx.transport.health() == super_platinum_core::slack::transport::Health::Offline {
        return Err("Reconnect to Slack before forwarding. Your note is still here.".into());
    }
    Ok(ctx)
}
#[derive(Clone, Copy)]
pub(super) struct Ui {
    pub state: Signal<ShellState>,
    pub deliveries: Signal<Deliveries>,
    pub saved: Signal<recovery::Saved>,
    pub selected: Signal<Vec<Destination>>,
    pub busy: Signal<bool>,
    pub error: Signal<String>,
    pub open_retry: Signal<u64>,
}
impl Ui {
    fn stop(mut self, error: Option<String>) {
        if let Ok(mut value) = self.busy.try_write() {
            *value = false;
        }
        if let Some(error) = error {
            if let Ok(mut value) = self.error.try_write() {
                *value = error;
            }
        }
    }
    fn current(self, target: &ForwardTarget, ctx: &Context) -> bool {
        self.state
            .try_peek()
            .ok()
            .is_some_and(|s| same_dialog(&s, target) && ctx.current(&s))
    }
    fn publish(mut self, receipt: Receipt) {
        let id = receipt.scope.destination.clone();
        if let Ok(mut saved) = self.saved.try_write() {
            saved.receipts.insert(id.clone(), receipt.clone());
        }
        if let Ok(mut ledger) = self.deliveries.try_write() {
            if receipt.retired {
                ledger.0.remove(&id);
            } else {
                ledger.0.insert(id, receipt.delivery);
            }
        }
    }
    fn storage(self) -> Option<recovery::Storage> {
        self.saved
            .try_peek()
            .ok()
            .filter(|s| s.loaded)
            .and_then(|s| s.storage.clone())
    }
    fn receipt(self, id: &str) -> Option<Receipt> {
        self.saved.try_peek().ok()?.receipts.get(id).cloned()
    }
    pub async fn submit(
        mut self,
        target: ForwardTarget,
        selected: Vec<Destination>,
        resend: Option<String>,
    ) {
        let ctx = match context(&self.state, &target) {
            Ok(ctx) => ctx,
            Err(error) => {
                self.stop(Some(error));
                return;
            }
        };
        let Some(storage) = self.storage() else {
            self.stop(Some(
                "Wait for saved forwards to load before sending.".into(),
            ));
            return;
        };
        let mut resolution_errors = Vec::new();
        let mut batch = Vec::new();
        if let Some(id) = resend {
            let Some(receipt) = self.receipt(&id) else {
                self.stop(Some("Reopen this forward to load its saved record.".into()));
                return;
            };
            if receipt.delivery.retry_at() > now() {
                self.stop(Some(format!(
                    "Wait {} seconds before resending.",
                    receipt.delivery.retry_at().saturating_sub(now())
                )));
                return;
            }
            batch.push((
                receipt.delivery.request.clone(),
                receipt.delivery.name.clone(),
                Some(receipt),
            ));
        } else {
            if self.saved.peek().mixed_notes() {
                self.stop(Some("Resolve the saved forwards with different notes before choosing another destination.".into()));
                return;
            }
            let Some(blocks) = read_note(&target.id).await else {
                self.stop(Some("The note could not be read. Please try again.".into()));
                return;
            };
            if !self.current(&target, &ctx) {
                return;
            }
            let mut outcome = self.deliveries.peek().clone();
            let opening_retry = *self.open_retry.peek();
            for destination in selected {
                if !self.current(&target, &ctx) {
                    return;
                }
                if destination.person.is_some() && now() < opening_retry {
                    resolution_errors.push(format!(
                        "Wait {} seconds before opening more DMs.",
                        opening_retry.saturating_sub(now())
                    ));
                    continue;
                }
                let original = destination.id.clone();
                match people::resolve(self.state, &ctx, destination).await {
                    Ok(destination) => {
                        if !self.current(&target, &ctx) {
                            return;
                        }
                        if let Ok(mut selected) = self.selected.try_write() {
                            for entry in selected.iter_mut().filter(|d| d.id == original) {
                                *entry = destination.clone();
                            }
                            let mut seen = std::collections::HashSet::new();
                            selected.retain(|d| seen.insert(d.id.clone()));
                        }
                        if let Some(request) = outcome.prepare(
                            &target.channel,
                            &target.message.ts,
                            &destination.id,
                            &destination.name,
                            &blocks,
                            now(),
                        ) {
                            batch.push((request, destination.name, None));
                        }
                    }
                    Err(error) => {
                        resolution_errors.push(error.message);
                        if let Some(seconds) = error.retry_after {
                            if let Ok(mut retry) = self.open_retry.try_write() {
                                *retry = now().saturating_add(seconds);
                            }
                            break;
                        }
                    }
                }
            }
        }
        let mut successful = Vec::new();
        for (request, name, previous) in batch {
            if !self.current(&target, &ctx) {
                return;
            }
            let started = if let Some(previous) = previous {
                storage.resend(previous).await
            } else {
                storage.begin(request, name).await.map(Some)
            };
            let receipt = match started {
                Ok(Some(Begin::Started(receipt))) => receipt,
                Ok(Some(Begin::Blocked(receipt))) => {
                    if self.current(&target, &ctx) {
                        self.publish(receipt);
                    }
                    resolution_errors.push(
                        "This forward is waiting for a delivery check or Slack’s retry delay."
                            .into(),
                    );
                    continue;
                }
                Ok(None) => {
                    resolution_errors.push(
                        "This forward changed in another view. Reopen it before trying again."
                            .into(),
                    );
                    continue;
                }
                Err(error) => {
                    resolution_errors.push(error);
                    break;
                }
            };
            // Intent is durable before dispatch. An abandoned intent remains
            // uncertain; reopening never sends it automatically.
            if !self.current(&target, &ctx) {
                return;
            }
            self.publish(receipt.clone());
            if let Ok(mut ledger) = self.deliveries.try_write() {
                if let Some(d) = ledger.0.get_mut(&receipt.scope.destination) {
                    d.status = Status::Sending;
                }
            }
            let result = api::forward_message(
                &ctx.transport,
                &ctx.client,
                &ctx.session,
                &receipt.delivery.request,
            )
            .await;
            // Keep a local rate-limit barrier even when persisting the response
            // fails; the already committed request remains uncertain.
            let mut receipt = receipt;
            if let Err(super_platinum_core::slack::Error::RateLimited { retry_after_secs }) =
                &result
            {
                receipt
                    .delivery
                    .defer_until(now().saturating_add(retry_after_secs.unwrap_or(30)));
            }
            // Always save to the captured account before any UI/account guard.
            let result = storage.finish(receipt.clone(), result).await;
            if !self.current(&target, &ctx) {
                return;
            }
            match result {
                Ok(Some(receipt)) => {
                    if matches!(receipt.delivery.status, Status::Sent(_)) {
                        successful.push(receipt.scope.destination.clone());
                    }
                    self.publish(receipt);
                }
                Ok(None) => {
                    self.publish(receipt);
                    resolution_errors.push("The delivery record is unavailable. Inspect the destination before resending.".into());
                }
                Err(error) => {
                    self.publish(receipt);
                    resolution_errors.push(error);
                }
            }
        }
        if !self.current(&target, &ctx) {
            return;
        }
        if let Ok(mut selected) = self.selected.try_write() {
            selected.retain(|d| !self.deliveries.peek().excluded(&d.id));
        }
        let done = resolution_errors.is_empty()
            && self.selected.peek().is_empty()
            && !self.deliveries.peek().0.is_empty()
            && self
                .deliveries
                .peek()
                .0
                .values()
                .all(|d| matches!(d.status, Status::Sent(_)));
        self.stop((!resolution_errors.is_empty()).then(|| resolution_errors.join(" ")));
        if done {
            close(self.state);
            self.state.write().show_toast("Message forwarded.");
        }
        if self
            .state
            .peek()
            .core
            .active_channel
            .as_ref()
            .is_some_and(|c| successful.contains(c))
        {
            crate::bootstrap::refresh_selected_channel(self.state).await;
        }
    }
    pub async fn check(self, target: ForwardTarget, id: String) {
        let ctx = match context(&self.state, &target) {
            Ok(ctx) => ctx,
            Err(error) => {
                self.stop(Some(error));
                return;
            }
        };
        let (Some(storage), Some(expected)) = (self.storage(), self.receipt(&id)) else {
            self.stop(Some("Reopen this forward to load its saved record.".into()));
            return;
        };
        // A second view can extend a cooldown or replace the attempt.
        if expected.delivery.retry_at() > now() {
            self.stop(Some(format!(
                "Wait {} seconds before checking delivery again.",
                expected.delivery.retry_at().saturating_sub(now())
            )));
            return;
        }
        let receipt = match storage.refresh(&expected).await {
            Ok(Some(receipt)) => receipt,
            Ok(None) => {
                self.stop(Some("The saved forward is no longer available.".into()));
                return;
            }
            Err(error) => {
                self.stop(Some(error));
                return;
            }
        };
        if !self.current(&target, &ctx) {
            return;
        }
        self.publish(receipt.clone());
        if receipt.attempt_id != expected.attempt_id || !receipt.unresolved() {
            self.stop(Some(
                "This forward has changed. Review its current delivery status.".into(),
            ));
            return;
        }
        if receipt.delivery.retry_at() > now() {
            self.stop(Some(format!(
                "Wait {} seconds before checking delivery again.",
                receipt.delivery.retry_at().saturating_sub(now())
            )));
            return;
        }
        let mut cursor = None;
        for _ in 0..3 {
            let result = api::fetch_history(
                &ctx.transport,
                &ctx.client,
                &ctx.session,
                api::HistoryArgs {
                    channel: id.clone(),
                    cursor: cursor.clone(),
                    oldest: Some(format!(
                        "{}.000000",
                        receipt.delivery.started_at.saturating_sub(5)
                    )),
                    limit: Some(100),
                    ..Default::default()
                },
            )
            .await;
            let page = match result {
                Ok(page) => page,
                Err(error) => {
                    let mut local = receipt.clone();
                    let saved = if let super_platinum_core::slack::Error::RateLimited {
                        retry_after_secs,
                    } = error
                    {
                        let seconds = retry_after_secs.unwrap_or(30);
                        local.delivery.defer_until(now().saturating_add(seconds));
                        Some(storage.defer(receipt.clone(), seconds).await)
                    } else {
                        None
                    };
                    if !self.current(&target, &ctx) {
                        return;
                    }
                    match saved {
                        Some(Ok(Some(receipt))) => self.publish(receipt),
                        Some(Err(error)) => {
                            self.publish(local);
                            self.stop(Some(error));
                            return;
                        }
                        Some(Ok(None)) => self.publish(local),
                        None => {}
                    }
                    self.stop(Some(
                        "Delivery could not be checked. Inspect the destination before resending."
                            .into(),
                    ));
                    return;
                }
            };
            let next = page
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());
            let saved = storage.confirm(receipt.clone(), page.messages).await;
            if !self.current(&target, &ctx) {
                return;
            }
            match saved {
                Ok(Some(current)) => {
                    let confirmed = matches!(current.delivery.status, Status::Sent(_));
                    let changed = current.attempt_id != receipt.attempt_id
                        || !current.unresolved()
                        || current.delivery.retry_at() > now();
                    self.publish(current);
                    if confirmed {
                        self.stop(None);
                        return;
                    }
                    if changed {
                        self.stop(Some(
                            "This saved forward changed while checking delivery.".into(),
                        ));
                        return;
                    }
                }
                Ok(None) => {
                    self.stop(Some("The saved forward is no longer available.".into()));
                    return;
                }
                Err(error) => {
                    self.stop(Some(error));
                    return;
                }
            }
            if next.is_none() || next == cursor {
                break;
            }
            cursor = next;
        }
        self.stop(Some("No matching receipt was found in recent messages. This does not prove the forward failed. Inspect the destination before resending.".into()));
    }
    pub async fn retire(self, target: ForwardTarget, id: String) {
        let (Some(storage), Some(receipt)) = (self.storage(), self.receipt(&id)) else {
            self.stop(Some("Reopen this forward to load its saved record.".into()));
            return;
        };
        let result = storage.retire(receipt).await;
        if !self
            .state
            .try_peek()
            .ok()
            .is_some_and(|s| same_dialog(&s, &target) && storage.current(&s))
        {
            return;
        }
        match result {
            Ok(Some(receipt)) => {
                let retired = receipt.retired;
                self.publish(receipt);
                self.stop(Some(
                    if retired {
                        "Tracking stopped. Delivery in Slack remains unconfirmed."
                    } else {
                        "This forward changed. Review its current status before stopping tracking."
                    }
                    .into(),
                ));
            }
            Ok(None) => self.stop(Some("The saved forward is no longer available.".into())),
            Err(error) => self.stop(Some(error)),
        }
    }
}

fn focus_action(id: &str, label: &str) {
    let id = serde_json::to_string(&format!("forward-result-{id}")).unwrap();
    let label = serde_json::to_string(label).unwrap();
    dioxus::document::eval(&format!(
        "requestAnimationFrame(()=>requestAnimationFrame(()=>{{const row=document.getElementById({id});[...row?.querySelectorAll('button')||[]].find(b=>b.textContent==={label})?.focus();}}));"
    ));
}

#[component]
pub(super) fn Results(
    state: Signal<ShellState>,
    deliveries: Signal<Deliveries>,
    saved: Signal<recovery::Saved>,
    selected: Signal<Vec<Destination>>,
    busy: Signal<bool>,
    error: Signal<String>,
    open_retry: Signal<u64>,
) -> Element {
    let mut confirm = use_signal(|| None::<String>);
    let mut retiring = use_signal(|| None::<String>);
    let outcomes = use_memo(move || {
        deliveries
            .read()
            .0
            .iter()
            .map(|(id, d)| (id.clone(), d.status.clone()))
            .collect::<Vec<_>>()
    });
    use_effect(move || {
        if !outcomes.read().is_empty() {
            dioxus::document::eval(
                "requestAnimationFrame(()=>{const root=document.getElementById('forward-dialog');const row=root?.querySelector('[data-delivery=uncertain], [data-delivery=failed]')||root?.querySelector('.forward-results');row?.scrollIntoView({block:'nearest'});});",
            );
        }
    });
    use_effect(move || {
        let resending = confirm();
        let stopping = retiring();
        if let Some(id) = stopping.or(resending) {
            let id = serde_json::to_string(&format!("forward-result-{id}")).unwrap();
            dioxus::document::eval(&format!(
                "requestAnimationFrame(()=>{{const row=document.getElementById({id});row?.querySelector('button')?.focus({{preventScroll:true}});row?.scrollIntoView({{block:'nearest'}});}});"
            ));
        }
    });
    let snapshot = deliveries.read();
    let target = state.read().forward_target.clone();
    let Some(target) = target else {
        return rsx! {};
    };
    let ui = Ui {
        state,
        deliveries,
        saved,
        selected,
        busy,
        error,
        open_retry,
    };
    rsx! {
        if !snapshot.0.is_empty() {
            section {class:"forward-results","aria-label":"Forwarding results",
                h3 {"Delivery"}
                for (id,delivery) in &snapshot.0 {
                    {let id=id.clone();let check_id=id.clone();let resend_id=id.clone();let confirm_id=id.clone();let target=target.clone();let check_target=target.clone();let retire_target=target.clone();let retire_id=id.clone();let retire_confirm=id.clone();let cancel_retire=id.clone();let cancel_resend=id.clone();
                    let (kind,label)=match &delivery.status {Status::Sending=>("sending","Forwarding…"),Status::Sent(_)=>("sent","Forwarded"),Status::Failed(message)=>("failed",message.as_str()),Status::Uncertain(message)=>("uncertain",message.as_str())};
                    rsx!{div {id:"forward-result-{id}",class:"forward-result","data-destination":id.clone(),"data-delivery":kind,
                        strong {"{delivery.name}"}
                        p {role:"status","{label}"}
                        if matches!(delivery.status,Status::Uncertain(_)) {
                            if !delivery.request.blocks.is_empty() {
                                details {class:"forward-saved-note",summary {{crate::icons::icon(crate::icons::Icon::ChevronRight,"icon sm")}"Original note"}
                                    {let shell=state.read();let nodes=shell.core.workspaces.get(&target.team).map(|w|delivery.request.blocks.iter().flat_map(|block|crate::blocks::block_nodes(crate::blocks::BlockCtx::new(w,&shell.media),block)).collect::<Vec<_>>()).unwrap_or_default();rsx!{for node in &nodes {{crate::view::rich::rich_node(node,shell.media_epoch,state)}}}}
                                }
                            }
                            if retiring.read().as_ref()==Some(&id) {
                                p {"Stop tracking this forward? Delivery in Slack will remain unconfirmed. A later forward could create a duplicate."}
                                div {class:"forward-recovery",
                                    button {disabled:busy(),onclick:move |_|{retiring.set(None);focus_action(&cancel_retire,"Stop tracking…");},"Cancel"}
                                    button {disabled:busy(),onclick:move |_|{if busy(){return;}busy.set(true);error.set(String::new());retiring.set(None);dioxus::core::spawn_forever(ui.retire(retire_target.clone(),retire_id.clone()));},"Stop tracking"}
                                }
                            } else
                            if confirm.read().as_ref()==Some(&id) {
                                p {"Slack may already have delivered this. Resending could create a duplicate."}
                                div {class:"forward-recovery",
                                    button {disabled:busy(),onclick:move |_|{confirm.set(None);focus_action(&cancel_resend,"Resend…");},"Cancel"}
                                    button {disabled:busy(),onclick:move |_|{if busy(){return;}busy.set(true);error.set(String::new());confirm.set(None);let target=target.clone();let id=resend_id.clone();dioxus::core::spawn_forever(ui.submit(target,vec![],Some(id)));},"Resend original"}
                                }
                            } else {
                                div {class:"forward-recovery",
                                    button {disabled:busy(),onclick:move |_|{if busy(){return;}busy.set(true);error.set(String::new());let target=check_target.clone();let id=check_id.clone();dioxus::core::spawn_forever(ui.check(target,id));},"Check delivery"}
                                    button {disabled:busy(),onclick:move |_|confirm.set(Some(confirm_id.clone())),"Resend…"}
                                    if saved.read().storage.is_some() {button {disabled:busy(),onclick:move |_|retiring.set(Some(retire_confirm.clone())),"Stop tracking…"}}
                                }
                            }
                        }
                    }}}
                }
            }
        }
    }
}

pub(super) fn fixture() -> Deliveries {
    let mut ledger = Deliveries::default();
    if crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_FIXTURE").is_ok_and(|s| s.starts_with("forward-partial"))
    {
        for (id, name) in [
            ("C1", "general"),
            ("C2", "ship"),
            ("C5", "private-planning-with-a-long-conversation-name"),
        ] {
            let request = ledger
                .prepare("C1", "100.000001", id, name, &[], now())
                .unwrap();
            let result = match id {
                "C1" => Ok(api::ForwardedMessage {
                    channel: id.into(),
                    ts: "101.000001".into(),
                }),
                "C2" => Err(super_platinum_core::slack::Error::Api(
                    "not_in_channel".into(),
                )),
                _ => Err(super_platinum_core::slack::Error::Offline("timeout".into())),
            };
            ledger.finish(&request, result, now());
        }
    }
    ledger
}
