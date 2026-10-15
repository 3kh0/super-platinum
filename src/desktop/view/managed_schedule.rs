use crate::{
    drafts::schedule::{self, Decision, Dialog, Request},
    icons::{Icon, icon},
    state::ShellState,
};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::drafts::DraftKey;

#[component]
pub(crate) fn RejectedSchedule(
    state: Signal<ShellState>,
    draft_key: DraftKey,
    onreview: EventHandler<Request>,
) -> Element {
    let eligible = Request::rejected(&state.read(), &draft_key).is_some();
    rsx! {
        div { class:"scheduled-actions",
            button { disabled:!eligible, onclick:move |_|{
                if let Some(request)=Request::rejected(&state.peek(), &draft_key){onreview.call(request);}
            }, "Review schedule" }
        }
    }
}

#[component]
pub(crate) fn ManagedScheduleDialog(
    state: Signal<ShellState>,
    request: Request,
    onclose: EventHandler,
) -> Element {
    let mut loaded = use_signal(|| None::<Result<Arc<Dialog>, crate::drafts::Fault>>);
    use_future(move || {
        let request = request.clone();
        async move {
            loaded.set(Some(schedule::open(state, request).await));
        }
    });
    use_drop(move || {
        if let Some(Ok(dialog)) = loaded.peek().as_ref() {
            dialog.close();
        }
    });
    match loaded() {
        Some(Ok(dialog)) => {
            rsx! { ScheduleReview { key:"{dialog.generation()}", state, dialog, onclose } }
        }
        result => rsx! {
            div {class:"scrim managed-schedule-scrim",onclick:move |_|onclose.call(())}
            section {class:"modal schedule-dialog managed-schedule-dialog",role:"dialog","aria-modal":"true","aria-label":"Schedule message",
                header {h2 {"Schedule message"}button {"aria-label":"Close scheduling",onclick:move |_|onclose.call(()),{icon(Icon::Close,"icon")}}}
                p {class:"scheduled-hint",role:"status", if result.is_none() {"Loading the saved draft…"} else {"This draft changed. Close this review and open its current saved version."} }
            }
        },
    }
}

#[component]
fn ScheduleReview(
    mut state: Signal<ShellState>,
    dialog: Arc<Dialog>,
    onclose: EventHandler,
) -> Element {
    let generation = dialog.generation();
    let initial_time = dialog.post_at();
    let mut date = use_signal(String::new);
    let mut editing = use_signal(|| !dialog.rejected());
    let mut busy = use_signal(|| false);
    let mut stale = use_signal(|| false);
    let mut feedback = use_signal(String::new);
    let cleanup = dialog.clone();
    use_drop(move || cleanup.close());
    let observed = dialog.clone();
    use_effect(move || {
        let shell = state.read();
        if !busy() && !observed.current(&shell) {
            observed.close();
            stale.set(true);
        }
    });
    let blocks = use_hook(|| serde_json::to_string(&dialog.snapshot().blocks).unwrap());
    let labels = use_hook(|| {
        state
            .peek()
            .core
            .workspaces
            .get(&dialog.key().team)
            .map(|workspace| {
                super::rich_editor::mention_labels(workspace, &dialog.snapshot().blocks)
            })
            .unwrap_or_else(|| "{}".into())
    });
    let destination =
        super::scheduled::destination_label(&state.peek(), &dialog.snapshot().destinations);
    let retry_valid = super_platinum_core::scheduled::validate_time(
        initial_time,
        super_platinum_core::state::now_secs(),
    )
    .is_ok();
    let close = dialog.clone();
    let escape = dialog.clone();
    let cancel = dialog.clone();
    let submit = dialog.clone();
    let rejected = dialog.rejected();
    rsx! {
        div {class:"scrim managed-schedule-scrim",onclick:move |_|if !busy(){close.close();onclose.call(())}}
        section {class:"modal schedule-dialog managed-schedule-dialog",id:"managed-schedule-dialog",role:"dialog","aria-modal":"true","aria-labelledby":"managed-schedule-title","aria-busy":busy(),"data-editor-generation":"{generation}",
            onkeydown:move |event|if event.key()==Key::Escape {event.stop_propagation();if !busy(){escape.close();onclose.call(())}},
            onmounted:move |_|{spawn(async move {
                let mut eval=dioxus::document::eval(&format!("const root=document.getElementById('managed-schedule-dialog');if(root?.dataset.editorGeneration==='{generation}'){{root.dataset.composing='false';root.addEventListener('compositionstart',()=>root.dataset.composing='true');root.addEventListener('compositionend',()=>root.dataset.composing='false');}}"));
                // Setup is scoped to this native mount; date update also checks
                // component liveness, so a retired dialog never adopts it.
                let _=&mut eval;
                if let Some(value)=super::new_reminder::local_date_value(initial_time).await {
                    if let Ok(mut current)=date.try_write() && current.is_empty(){*current=value;}
                }
            });},
            header {h2 {id:"managed-schedule-title",if rejected {"Review schedule"} else {"Schedule message"}}button {"aria-label":"Close scheduling",disabled:busy(),onclick:move |_|{cancel.close();onclose.call(())},{icon(Icon::Close,"icon")}}}
            p {class:"scheduled-hint","To {destination}"}
            div {id:"managed-schedule-body",class:"managed-schedule-body",
            super::rich_editor::RichEditor {id:"managed-schedule-message",label:"Message",blocks,labels,disabled:busy()||!editing(),autofocus:!rejected,expected_generation:Some(generation)}
            div {class:"managed-schedule-time-fields",
            label {r#for:"managed-schedule-date","Date and time"}
            input {id:"managed-schedule-date",r#type:"datetime-local",value:"{date}",disabled:busy()||!editing(),oninput:move |event|date.set(event.value())}
            p {id:"managed-schedule-hint",class:"scheduled-hint",if rejected {"Uses your local time zone. Your separate composer draft will be kept."} else {"Uses your local time zone. Newer typing will be kept."}}
            if rejected && !editing() && !retry_valid {p {class:"scheduled-hint","This time is no longer available. Choose Change schedule to select a future time."}}
            if stale() {p {class:"later-error",role:"alert","This review is no longer current. Close it and review the saved draft again."}}
            if !feedback.read().is_empty() {p {class:"scheduled-hint",role:"status","{feedback}"}}
            }
            }
            footer {
                if rejected && !editing() {button {disabled:busy()||stale(),onclick:move |_|editing.set(true),"Change schedule"}}
                button {class:"primary",disabled:busy()||stale()||(editing()&&date.read().is_empty())||(!editing()&&!retry_valid),onclick:move |_|{
                    if busy()||stale(){return;}busy.set(true);feedback.set(String::new());
                    let dialog=submit.clone();let replace=editing();let date_value=date();
                    dioxus::core::spawn_forever(async move {
                        let result=async {
                            let receipt=read_native(generation,&date_value).await?;
                            let decision=if rejected&&!replace {Decision::Retry} else {
                                if rejected {Decision::Replace{blocks:receipt.0,post_at:receipt.1}} else {Decision::Create{blocks:receipt.0,post_at:receipt.1}}
                            };
                            schedule::submit(state,dialog.clone(),decision).await.map_err(|_|"This schedule needs review in Drafts. Nothing was automatically retried.")
                        }.await;
                        match result {
                            Ok(message)=>{
                                if let Ok(mut shell)=state.try_write() && dialog.feedback_current(&shell){shell.show_toast(message);}
                                dialog.close();
                                if busy.try_read().is_ok(){onclose.call(());}
                            }
                            Err(message)=>{if let Ok(mut value)=feedback.try_write(){*value=message.into();}}
                        }
                        if let Ok(mut value)=busy.try_write(){*value=false;}
                    });
                },if busy(){"Saving…"}else if rejected&&!editing(){"Retry same schedule"}else if rejected{"Save new schedule"}else{"Schedule message"}}
            }
        }
    }
}

async fn read_native(
    generation: u64,
    date: &str,
) -> Result<(Vec<serde_json::Value>, i64), &'static str> {
    let date = serde_json::to_string(date).map_err(|_| "The schedule time could not be read.")?;
    let mut eval = dioxus::document::eval(&format!(
        "try{{const root=document.getElementById('managed-schedule-dialog');if(root?.dataset.editorGeneration!=='{generation}'||root.dataset.composing!=='false'){{dioxus.send(null);}}else{{const time=Date.parse({date});dioxus.send({{generation:'{generation}',time:Number.isFinite(time)?Math.floor(time/1000):0,blocks:window.superPlatinumRichEditor.read('managed-schedule-message')}});}}}}catch{{dioxus.send(null);}}await dioxus.recv();"
    ));
    let receipt = eval.recv::<serde_json::Value>().await.ok();
    let _ = eval.send(());
    let value = receipt.ok_or("The message could not be read. Finish composing and try again.")?;
    if value.get("generation").and_then(|v| v.as_str()) != Some(&generation.to_string()) {
        return Err("This editor changed. Open a fresh review.");
    }
    let blocks = value
        .get("blocks")
        .and_then(|v| v.as_array())
        .cloned()
        .ok_or("The message could not be read.")?;
    let time = value
        .get("time")
        .and_then(|v| v.as_i64())
        .ok_or("Choose a future schedule time.")?;
    super_platinum_core::scheduled::validate_time(time, super_platinum_core::state::now_secs())?;
    Ok((blocks, time))
}
