//! Saved delivery remains reachable even without the original message in cache.
use super::*;
use super_platinum_core::forwarding::store::Receipt;

#[component]
pub(crate) fn SavedForwards(mut state: Signal<ShellState>) -> Element {
    let scope = use_memo(move || {
        let shell = state.read();
        (
            shell.core.active_account.clone(),
            shell.core.active_team.clone(),
            shell.core.account_epoch,
            shell.overlay == Some(Overlay::Forward),
        )
    });
    let mut refresh = use_signal(|| 0u64);
    let mut rows = use_signal(Vec::<Receipt>::new);
    let mut origin = use_signal(|| None::<(recovery::Storage, u64)>);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut generation = use_signal(|| 0u64);
    use_effect(move || {
        let current = scope();
        let _ = refresh();
        if current.3 {
            return;
        }
        let storage = recovery::Storage::capture(&state.peek());
        let Some(storage) = storage else {
            rows.set(vec![]);
            return;
        };
        let request = generation.peek().wrapping_add(1);
        generation.set(request);
        loading.set(true);
        error.set(String::new());
        rows.set(vec![]);
        spawn(async move {
            let result = storage.unresolved().await;
            if scope.peek().clone() != current
                || *generation.peek() != request
                || !storage.current(&state.peek())
            {
                return;
            }
            loading.set(false);
            match result {
                Ok(receipts) => {
                    origin.set(Some((storage, current.2)));
                    rows.set(receipts);
                }
                Err(message) => error.set(message),
            }
        });
    });
    if rows.read().is_empty() && error.read().is_empty() && !loading() {
        return rsx! {};
    }
    rsx! {
        section {id:"saved-forwards",class:"scheduled-item","aria-label":"Saved forwarding delivery",
            div {class:"scheduled-meta",strong {"Unconfirmed forwards"}span {class:"spacer"}
                button {disabled:loading(),onclick:move |_|refresh+=1,"Refresh"}
            }
            if loading() {p {role:"status",class:"scheduled-hint","Checking saved forwarding records…"}}
            if !error.read().is_empty() {p {role:"alert",class:"scheduled-hint","{error}"}}
            for receipt in rows.read().iter() {
                {let receipt=receipt.clone();let id=receipt.attempt_id.clone();let origin=origin.peek().clone();
                let shell=state.read();let source=shell.core.workspaces.get(&receipt.scope.team_id).and_then(|w|w.channels.get(&receipt.scope.source).map(|c|super_platinum_core::state::channel_display_name(w,c))).unwrap_or_else(||receipt.scope.source.clone());
                let date=super_platinum_core::state::format_ts_date_label(&receipt.scope.timestamp);let time=super_platinum_core::state::format_ts_hm(&receipt.scope.timestamp);
                rsx!{
                    div {class:"forward-result",key:"{id}","data-saved-forward":id,
                        strong {"To {receipt.delivery.name}"}
                        p {class:"scheduled-hint","From {source} · {date} {time}"}
                        p {class:"scheduled-hint","Delivery could not be confirmed. Review before resending."}
                        button {onclick:move |_|{
                            let mut shell=state.write();
                            if !origin.as_ref().is_some_and(|(storage,epoch)|*epoch==shell.core.account_epoch && storage.current(&shell)) {return;}
                            let id=api::ForwardMessage::new(String::new(),String::new(),String::new(),vec![]).client_msg_id;
                            shell.forward_target=Some(ForwardTarget {
                                recovery_only:true,epoch:shell.core.account_epoch,team:receipt.scope.team_id.clone(),channel:receipt.scope.source.clone(),
                                message:MessageVm {ts:receipt.scope.timestamp.clone(),..Default::default()},id,
                            });
                            shell.overlay=Some(Overlay::Forward);
                        },"Review delivery"}
                    }
                }}
            }
        }
    }
}
