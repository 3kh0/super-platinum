use super::*;

fn editor_owner(id: &str, revision: u64, visit: u64) -> String {
    format!("{id}:{revision}:{visit}")
}

fn mount_focus(config: serde_json::Value) {
    spawn(async move {
        let script = format!(
            "{};mountScheduledForwardFocus({config});",
            include_str!("focus.js")
        );
        let _ = document::eval(&script).await;
    });
}

async fn date_value(seconds: i64) -> Option<String> {
    let mut eval = document::eval(&format!(
        "const d=new Date({seconds}*1000);const p=n=>String(n).padStart(2,'0');dioxus.send(`${{d.getFullYear()}}-${{p(d.getMonth()+1)}}-${{p(d.getDate())}}T${{p(d.getHours())}}:${{p(d.getMinutes())}}`);"
    ));
    tokio::time::timeout(std::time::Duration::from_secs(2), eval.recv::<String>())
        .await
        .ok()?
        .ok()
}

#[component]
pub(crate) fn Controls(
    state: Signal<ShellState>,
    selected: Signal<Vec<Destination>>,
    mut busy: Signal<bool>,
    mut error: Signal<String>,
    dialog_id: String,
    private: bool,
) -> Element {
    let Some(book) = try_consume_context::<Signal<Registry>>() else {
        return rsx! {};
    };
    let mut expanded = use_signal(layout_fixture);
    let mut date = use_signal(String::new);
    let mut confirmed = use_signal(|| false);
    let mut retained = use_signal(|| None::<String>);
    use_effect(move || {
        if let Some(id) = retained() {
            let running = book.read().entries.get(&id).is_some_and(|entry| entry.busy);
            if *busy.peek() != running {
                busy.set(running);
            }
        }
    });
    let focus_owner = dialog_id.clone();
    let summary = retained.read().as_ref().and_then(|id| {
        book.read().entries.get(id).map(|entry| {
            if !entry.error.is_empty() {
                entry.error.clone()
            } else {
                entry
                    .record
                    .as_ref()
                    .map(|r| r.outcome().to_owned())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Saving your scheduled forward…".into())
            }
        })
    });
    rsx! {
        div {class:"forward-schedule",onmounted:move |_|{
            mount_focus(serde_json::json!({"root":"forward-dialog","ownerKey":"scheduledForwardOwner","owner":focus_owner,"scope":".forward-schedule","target":".forward-schedule [data-scheduled-focus-target]","fallback":"[aria-label=\"Close forward dialog\"]"}));
            if layout_fixture(){spawn(async move {if let Some(value)=date_value(super_platinum_core::state::now_secs()+3600).await {if date.peek().is_empty(){date.set(value);}}});}
        },
            if !expanded() {button {disabled:busy(),onclick:move |_|{
                expanded.set(true);
                spawn(async move {if let Some(value)=date_value(super_platinum_core::state::now_secs()+3600).await {if date.peek().is_empty(){date.set(value);}}});
            },"Schedule later"}}
            else {
                label {r#for:"forward-schedule-date","Date and time"}
                input {id:"forward-schedule-date",r#type:"datetime-local",value:"{date}",disabled:busy()||retained.read().is_some(),oninput:move |event|date.set(event.value())}
                p {class:"forward-hint","Uses your local time zone. Choose one destination to schedule this forward."}
                if private {label {class:"schedule-toggle",input {r#type:"checkbox",checked:confirmed(),disabled:busy()||retained.read().is_some(),onchange:move |event|confirmed.set(event.checked())}span {"I understand recipients will see this private message."}}}
                button {disabled:busy()||retained.read().is_some()||selected.read().len()!=1||date.read().is_empty()||(private&&!confirmed()),onclick:move |_|{
                    if busy()||retained.peek().is_some(){return;}
                    let Some(destination)=selected.peek().first().cloned().filter(|_|selected.peek().len()==1)else{return;};
                    let Some(target)=state.peek().forward_target.clone().filter(|target|target.id==dialog_id && same_dialog(&state.peek(),target))else{return;};
                    let owner=match Owner::capture(state){Ok(owner)=>owner,Err(message)=>{error.set(message);return;}};
                    let private_confirmed=confirmed();busy.set(true);error.set(String::new());
                    dioxus::core::spawn_forever(async move {
                        let result=async {
                            let (note,time)=capture::read("forward-dialog","forward-note","forward-schedule-date",&target.id,false).await?;

                            let source=model::Source {
                                channel:target.channel.clone(),timestamp:target.message.ts.clone(),thread:target.message.thread_ts.clone().filter(|root|root!=&target.message.ts),author:target.message.user_id.clone(),
                                permalink:api::workspace_permalink(&owner.context.session.url,&target.channel,&target.message.ts,target.message.thread_ts.as_deref().filter(|root|*root!=target.message.ts)).ok_or("The source message link could not be verified.")?,private,
                            };
                            let destination_id=if let Some(person)=&destination.person {model::Destination::Person(person.id.clone())}else{model::Destination::Conversation(destination.id.clone())};
                            let intent=model::Intent::new(owner.storage.owner.clone(),source,destination_id,destination.name,note,time,private_confirmed,now() as i64).map_err(|_|"Choose a valid future time and confirm private forwarding.".to_owned())?;
                            controller::create(state,book,owner,intent)
                        }.await;
                        match result {Ok(id)=>{if let Ok(mut slot)=retained.try_write(){*slot=Some(id);}},Err(message)=>{if let Ok(mut slot)=busy.try_write(){*slot=false;}if let Ok(mut slot)=error.try_write(){*slot=message;}}}
                    });
                },"Schedule forward"}
                if retained.read().is_none() {button {disabled:busy(),onclick:move |_|expanded.set(false),"Cancel scheduling"}}
            }
            if expanded() {p {class:"forward-hint scheduled-forward-focus-status",tabindex:"-1","data-scheduled-focus-target":"true",role:"status",{summary.clone().unwrap_or_else(||"Choose a time, then schedule this forward.".into())}}}
            if summary.is_some() {p {class:"forward-hint","Review the complete saved record in Drafts & scheduled."}}
        }
    }
}

#[component]
pub(crate) fn Inventory(state: Signal<ShellState>) -> Element {
    let Some(mut book) = try_consume_context::<Signal<Registry>>() else {
        return rsx! {};
    };
    let focus_visit = use_hook(next_editor_visit);
    let scope = use_memo(move || {
        let shell = state.read();
        (
            shell.core.active_account.clone(),
            shell.core.active_team.clone(),
            shell.core.account_epoch,
        )
    });
    let mut cursor = use_signal(|| None::<String>);
    let mut previous = use_signal(Vec::<Option<String>>::new);
    let mut refresh = use_signal(|| 0u64);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut generation = use_signal(|| 0u64);
    let mut page = use_signal(Vec::<String>::new);
    let mut selected = use_signal(|| None::<String>);
    use_effect(move || {
        let _ = scope();
        cursor.set(None);
        previous.set(vec![]);
        selected.set(None);
    });
    use_effect(move || {
        let current = scope();
        let after = cursor();
        let _ = refresh();
        let Some(storage) = storage::Storage::capture(&state.peek()) else {
            return;
        };
        let request = generation.peek().wrapping_add(1);
        generation.set(request);
        loading.set(true);
        error.set(String::new());
        spawn(async move {
            let result = storage.page(after).await;
            if *generation.peek() != request
                || *scope.peek() != current
                || !storage.current(&state.peek())
            {
                return;
            }
            loading.set(false);
            match result {
                Ok(records) => {
                    page.set(
                        records
                            .iter()
                            .map(|record| record.id().to_owned())
                            .collect(),
                    );
                    book.write().adopt(storage, records);
                }
                Err(message) => error.set(message),
            }
        });
    });
    let Some(storage) = storage::Storage::capture(&state.read()) else {
        return rsx! {};
    };
    let rows = book.read().snapshots(&storage);
    if rows.is_empty() && !loading() && error.read().is_empty() {
        return rsx! {};
    }
    rsx! {
        section {id:"scheduled-forwards",class:"scheduled-item","aria-label":"Scheduled forwards","data-scheduled-focus-owner":"{focus_visit}",onmounted:move |_|{
            mount_focus(serde_json::json!({"root":"scheduled-forwards","ownerKey":"scheduledFocusOwner","owner":focus_visit.to_string(),"target":"[data-scheduled-inventory-focus]"}));
        },
            div {class:"scheduled-meta",strong {class:"scheduled-forward-focus-status",tabindex:"-1","data-scheduled-inventory-focus":"true","Scheduled forwards"}span {class:"spacer"}button {disabled:loading()||selected.read().is_some(),onclick:move |_|{if selected.peek().is_none(){refresh+=1;}},"Refresh records"}}
            if loading(){p {class:"scheduled-hint",role:"status","Reading saved forwards…"}}
            if !error.read().is_empty(){p {class:"later-error",role:"alert","{error}"}}
            {rows.into_iter().map(|row| history_row(state,book,selected,loading(),row))}
            div {class:"scheduled-meta",
                button {disabled:loading()||selected.read().is_some()||previous.read().is_empty(),onclick:move |_|{let prior=previous.write().pop();if let Some(prior)=prior {cursor.set(prior);selected.set(None);}},"Previous records"}
                button {disabled:loading()||selected.read().is_some()||page.read().len()<50,onclick:move |_|{let next=page.peek().last().cloned();if next.is_some(){previous.write().push(cursor());cursor.set(next);selected.set(None);}},"Next records"}
            }
        }
    }
}

fn history_row(
    state: Signal<ShellState>,
    book: Signal<Registry>,
    mut selected: Signal<Option<String>>,
    loading: bool,
    row: Snapshot,
) -> Element {
    let id = row.id.clone();
    let retry = id.clone();
    let check = id.clone();
    let check_label = if row.continuing {
        "Continue checking older drafts"
    } else {
        "Check scheduled forward"
    };
    let open = id.clone();
    let record = row.record.clone();
    let outcome = record
        .as_ref()
        .map(|r| r.outcome().to_owned())
        .unwrap_or_else(|| {
            "Your complete input is retained locally until its record can be saved.".into()
        });
    let reviewable = record
        .as_ref()
        .is_some_and(|r| matches!(r.phase(), model::Phase::Unknown | model::Phase::Conflict));
    let editable = record.as_ref().is_some_and(|r| {
        matches!(
            r.phase(),
            model::Phase::Ready | model::Phase::Rejected | model::Phase::Confirmed
        )
    });
    rsx! {div {class:"forward-result",key:"{id}","data-scheduled-forward":id.clone(),
        strong {"To {row.intent.destination_name()}"}
        p {class:"scheduled-hint","From {row.intent.source().channel} · {row.intent.source().timestamp}"}
        p {class:"scheduled-hint","{outcome}"}
        if !row.error.is_empty(){p {class:"later-error",role:"alert","{row.error}"}}
        if row.needs_storage {button {disabled:row.busy,onclick:move |_|controller::retry_storage(state,book,retry.clone()),"Retry saving record"}}
        else if reviewable {button {disabled:row.busy||record.as_ref().is_some_and(|r|now()<r.retry_at()),onclick:move |_|controller::review(state,book,check.clone()),"{check_label}"}}
        if editable || reviewable {button {disabled:row.busy||loading||selected.read().is_some(),onclick:move |_|{if !loading && selected.peek().is_none() {selected.set(Some(open.clone()));}},"Review note and time"}}
        if selected.read().as_ref()==Some(&id) {Editor {key:"{id}",state,record_id:id.clone(),onclose:move |_|selected.set(None)}}
    }}
}

#[component]
fn Editor(state: Signal<ShellState>, record_id: String, onclose: EventHandler) -> Element {
    let book = consume_context::<Signal<Registry>>();
    let visit = use_hook(next_editor_visit);
    let mut date = use_signal(String::new);
    let capturing = use_signal(|| false);
    let error = use_signal(String::new);
    let mut confirm_resolve = use_signal(|| None::<u64>);
    let mut confirm_delete = use_signal(|| None::<u64>);
    let Some(row) = book.read().entries.get(&record_id).map(|entry| {
        (
            entry.intent.clone(),
            entry.record.clone(),
            entry.busy,
            entry.completion.is_some() || entry.unsent.is_some() || entry.local.is_some(),
            entry.pending_edit.clone(),
        )
    }) else {
        return rsx! {};
    };
    let (intent, record, running, pending_storage, pending_edit) = row;
    let editable = record.as_ref().is_some_and(|r| {
        matches!(
            r.phase(),
            model::Phase::Ready | model::Phase::Rejected | model::Phase::Confirmed
        )
    });
    let busy = running || capturing();
    let blocks = serde_json::to_string(
        pending_edit
            .as_ref()
            .map(|(note, _)| note.as_slice())
            .unwrap_or_else(|| intent.note()),
    )
    .unwrap_or_else(|_| "[]".into());
    let time = pending_edit
        .as_ref()
        .map(|(_, time)| *time)
        .unwrap_or_else(|| intent.post_at());
    let save = record_id.clone();
    let cancel = record_id.clone();
    let delete_record = record.clone();
    let resolve_record = record.clone();
    let revision = record.as_ref().map(|record| record.revision());
    let owner = editor_owner(
        &record_id,
        record.as_ref().map(|r| r.revision()).unwrap_or(0),
        visit,
    );
    let draft_owner = owner.clone();
    let save_owner = owner.clone();
    rsx! {
        section {id:"scheduled-forward-edit","data-editor-generation":"{visit}","data-scheduled-forward-owner":owner.clone(),"data-scheduled-forward-busy":busy,class:"schedule-dialog","aria-label":"Review scheduled forward",
            onmounted:move |_|{spawn(async move {if let Some(value)=date_value(if time>0{time}else{super_platinum_core::state::now_secs()+3600}).await {if date.peek().is_empty(){date.set(value);}}});},
            p {class:"scheduled-hint scheduled-forward-focus-status",tabindex:"-1","data-scheduled-focus-target":"true",role:"status",if busy {"Saving the scheduled forward…"}else if editable {"Review your note and time."}else {"Your requested forward is retained for review."}}
            crate::view::rich_editor::RichEditor {id:"scheduled-forward-note",label:"Forwarding note",blocks,disabled:busy||!editable||pending_storage,autofocus:false,expected_generation:Some(visit)}
            p {class:"scheduled-hint","Source: {intent.source().permalink}"}
            label {r#for:"scheduled-forward-date","Date and time"}
            input {id:"scheduled-forward-date",r#type:"datetime-local",value:"{date}",disabled:busy||!editable||pending_storage,oninput:move |event|date.set(event.value())}
            if !editable {p {class:"scheduled-hint","Your requested note and the source are retained. Check the scheduled forward before another write."}}
            if let Some(observed)=record.as_ref().and_then(|record|record.observed()).filter(|_|record.as_ref().is_some_and(|r|r.phase()==model::Phase::Conflict)) {
                {observed_preview(state, &intent.owner().team, observed)}
                if confirm_resolve()!=revision || revision.is_none(){button {disabled:busy||pending_storage,onclick:move |_|confirm_resolve.set(revision),"Keep my requested note and time…"}}
                else {
                    p {class:"scheduled-hint","Keep your requested forward instead of the shown Slack draft? A later Save will replace its message, destination and time with your requested forward. This choice does not send anything."}
                    if observed["draft"]["file_ids"].as_array().is_some_and(|files| !files.is_empty()) {p {class:"scheduled-hint","Saving your requested forward will also remove the attached files shown above."}}
                    button {disabled:busy,onclick:move |_|confirm_resolve.set(None),"Keep both versions for now"}
                    button {disabled:busy||pending_storage,onclick:move |_|{if let Some(expected)=resolve_record.clone(){controller::resolve_conflict(state,book,expected);}},"Review and keep my requested version"}
                }
            }
            if !error.read().is_empty(){p {class:"later-error",role:"alert","{error}"}}
            div {class:"scheduled-meta",
                button {disabled:busy,onclick:move |_|onclose.call(()),if editable {"Cancel edits"}else{"Close review"}}
                if editable {button {disabled:busy||pending_storage||date.read().is_empty(),onclick:move |_|{
                    save_editor(state,book,save.clone(),save_owner.clone(),visit,false,capturing,error);
                },"Save scheduled forward"}}
                if editable&&record.as_ref().and_then(|r|r.server_id()).is_some() {
                    button {disabled:busy||pending_storage,onclick:move |_|{save_editor(state,book,cancel.clone(),draft_owner.clone(),visit,true,capturing,error);},"Cancel schedule; keep draft"}
                    if confirm_delete()!=revision || revision.is_none(){button {disabled:busy||pending_storage,onclick:move |_|confirm_delete.set(revision),"Delete scheduled draft…"}}
                    else {span {"Delete this scheduled draft?"}button {disabled:busy,onclick:move |_|confirm_delete.set(None),"Keep draft"}button {disabled:busy||pending_storage,onclick:move |_|{if let Some(expected)=delete_record.clone(){controller::mutate(state,book,expected,true,None);}},"Delete draft"}}
                }
            }
        }
    }
}

fn observed_preview(
    state: Signal<ShellState>,
    team: &str,
    observed: &serde_json::Value,
) -> Element {
    let draft = &observed["draft"];
    let (nodes, destinations, epoch) = {
        let shell = state.read();
        let Some(workspace) = shell.core.workspaces.get(team) else {
            return rsx! {p {"The saved Slack version is unavailable in this workspace."}};
        };
        let nodes = draft["blocks"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|block| {
                crate::blocks::block_nodes(
                    crate::blocks::BlockCtx::new(workspace, &shell.media),
                    block,
                )
            })
            .collect::<Vec<_>>();
        let mut destinations = Vec::new();
        for destination in draft["destinations"].as_array().into_iter().flatten() {
            if let Some(id) = destination["channel_id"].as_str() {
                destinations.push(
                    workspace
                        .channels
                        .get(id)
                        .map(|channel| {
                            super_platinum_core::state::channel_display_name(workspace, channel)
                        })
                        .unwrap_or_else(|| id.to_owned()),
                );
            }
            for id in destination["user_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
            {
                destinations.push(workspace.display_name(id));
            }
        }
        (nodes, destinations.join(", "), shell.media_epoch)
    };
    let time = draft["date_scheduled"].as_i64().unwrap_or(0);
    let files = draft["file_ids"].as_array().map_or(0, Vec::len);
    rsx! {
        details {summary {"Observed version from Slack"}
            p {class:"scheduled-hint","To {destinations}"}
            if time>0 {time {"{super_platinum_core::state::format_ts_date_label(&time.to_string())} · {super_platinum_core::state::format_ts_hm(&time.to_string())}"}} else {p {"Not scheduled"}}
            div {class:"forward-preview",for node in &nodes {{crate::view::rich::rich_node(node,epoch,state)}}}
            if files>0 {p {class:"scheduled-hint","Attached files: {files}"}}
        }
    }
}

fn save_editor(
    state: Signal<ShellState>,
    book: Signal<Registry>,
    id: String,
    native_owner: String,
    visit: u64,
    as_draft: bool,
    mut capturing: Signal<bool>,
    mut error: Signal<String>,
) {
    if *capturing.peek() {
        return;
    }
    let Some(expected) = book
        .peek()
        .entries
        .get(&id)
        .and_then(|entry| entry.record.clone())
    else {
        return;
    };
    if native_owner != editor_owner(&id, expected.revision(), visit) {
        error.set("The saved record changed. Reopen its note and time before saving.".into());
        return;
    }
    let owner = match Owner::capture(state) {
        Ok(owner) => owner,
        Err(message) => {
            error.set(message);
            return;
        }
    };
    let token = match controller::begin_capture(book, &owner, &expected) {
        Ok(token) => token,
        Err(message) => {
            error.set(message);
            return;
        }
    };
    capturing.set(true);
    dioxus::core::spawn_forever(async move {
        let result = capture::read(
            "scheduled-forward-edit",
            "scheduled-forward-note",
            "scheduled-forward-date",
            &native_owner,
            as_draft,
        )
        .await;
        if let Ok(mut value) = capturing.try_write() {
            *value = false;
        }
        match result {
            Ok((note, time)) => {
                controller::captured_edit(state, book, owner, expected, token, note, time)
            }
            Err(message) => {
                controller::cancel_capture(book, &id, &token, message.clone());
                if let Ok(mut value) = error.try_write() {
                    *value = message;
                }
            }
        }
    });
}

#[cfg(test)]
mod visit_tests {
    use super::*;
    #[test]
    fn reopened_editor_and_changed_record_have_distinct_capture_owners() {
        let first = next_editor_visit();
        let reopened = next_editor_visit();
        assert_ne!(first, reopened);
        assert_ne!(
            editor_owner("record", 4, first),
            editor_owner("record", 4, reopened)
        );
        assert_ne!(
            editor_owner("record", 4, first),
            editor_owner("record", 5, first)
        );
    }
}
