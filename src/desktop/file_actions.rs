//! Scoped actions for a file row. Mutations outlive an unmounted row.
use crate::{scheduled::Context, state::ShellState};
use dioxus::prelude::*;
use super_platinum_core::{
    file_actions as policy,
    operation_receipts::{Operation, Outcome},
    slack::{Error, api, models::File},
    state::file_title,
};
pub(crate) mod later;
mod recovery;
mod recovery_check;
pub(crate) mod recovery_list;
mod rename;
mod rename_check;

#[derive(Clone, PartialEq)]
pub struct Target {
    pub id: String,
    pub title: String,
    pub link: Option<String>,
    pub owned: bool,
    pub renameable: bool,
    pub epoch: u64,
    pub team: String,
}
impl Target {
    pub fn for_viewer(shell: &ShellState, viewer: &crate::state::ViewerVm) -> Option<Self> {
        let file = viewer.file.as_ref()?;
        let team = shell.core.active_team.as_ref()?;
        let workspace = shell.core.workspaces.get(team)?;
        if workspace.deleted_files.contains(&file.id) {
            return None;
        }
        Some(Self {
            id: file.id.clone(),
            title: viewer.name.clone(),
            link: file.permalink.clone(),
            owned: !workspace.self_user_id.is_empty()
                && file.uploader.as_deref() == Some(&workspace.self_user_id),
            renameable: file.renameable,
            epoch: shell.core.account_epoch,
            team: team.clone(),
        })
    }
    pub fn new(shell: &ShellState, file: &File) -> Self {
        let team = shell.core.active_team.clone().unwrap_or_default();
        let user = shell
            .core
            .workspaces
            .get(&team)
            .map(|w| w.self_user_id.as_str())
            .unwrap_or_default();
        Self {
            id: file.id.clone().unwrap_or_default(),
            title: file_title(file),
            link: policy::permalink(file),
            owned: policy::can_delete(file, user),
            renameable: policy::renameable(file),
            epoch: shell.core.account_epoch,
            team,
        }
    }
    fn current(&self, shell: &ShellState) -> bool {
        shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_deref() == Some(&self.team)
    }
}

#[derive(Default, Clone)]
pub struct ActionState {
    pub confirm: bool,
    pub busy: bool,
    pub uncertain: bool,
    pub error: Option<String>,
    pub rename: Option<rename::Edit>,
    pub blocked: bool,
    pub retry_at: u64,
    pub receipt: Option<super_platinum_core::operation_receipts::Receipt>,
    pub retire_confirm: bool,
}

pub async fn copy(mut state: Signal<ShellState>, target: Target) {
    if !target.current(&state.peek()) {
        return;
    }
    let Some(link) = &target.link else {
        return;
    };
    let json = serde_json::to_string(link).unwrap();
    let mut eval = document::eval(&format!(
        "await (async()=>{{let copied=false;try{{await navigator.clipboard.writeText({json});copied=true;}}catch{{}}dioxus.send(copied);await dioxus.recv();}})();"
    ));
    let copied = eval.recv::<bool>().await.unwrap_or(false);
    let _ = eval.send("received");
    if target.current(&state.peek()) {
        state.write().show_toast(if copied {
            "File link copied."
        } else {
            "The clipboard could not be updated. Please try again."
        });
    }
}

pub fn open(mut state: Signal<ShellState>, target: &Target) {
    if !target.current(&state.peek()) {
        return;
    }
    if let Some(link) = &target.link {
        if crate::media::open_external(link).is_err() {
            state
                .write()
                .show_toast("This file link could not be opened.");
        }
    }
}

fn finish(mut ui: Signal<ActionState>, message: &str, uncertain: bool) {
    if let Ok(mut action) = ui.try_write() {
        action.busy = false;
        action.uncertain = uncertain;
        action.error = Some(message.into());
    }
}

fn back(mut ui: Signal<ActionState>) {
    let retry_at = ui.peek().retry_at;
    ui.set(ActionState {
        retry_at,
        ..Default::default()
    });
}

fn removed(mut state: Signal<ShellState>, target: &Target) {
    let mut shell = state.write();
    shell.core.remove_file(&target.team, &target.id);
    // Cached renderer projections are rebuilt from the reconciled core messages.
    shell.refresh_from_core();
    shell.show_toast("File deleted.");
}

pub async fn delete(
    mut state: Signal<ShellState>,
    target: Target,
    mut ui: Signal<ActionState>,
    check_only: bool,
) {
    if !target.current(&state.peek()) {
        return;
    }
    if ui.try_peek().is_ok_and(|ui| ui.retry_at > recovery::now()) {
        finish(
            ui,
            "Slack asked us to wait before checking this change again.",
            check_only,
        );
        return;
    }
    let Some(ctx) = Context::get(&state) else {
        finish(ui, "Reconnect to Slack to manage this file.", check_only);
        return;
    };
    let mut pending = recovery::pending(state, &target, ui);
    if let Some(previous) = pending.as_ref() {
        let Some(current) = recovery::refresh(state, &target, ui, previous).await else {
            return;
        };
        pending = Some(current);
        if !ctx.current(&state.peek()) {
            return;
        }
    }
    let read_revision = crate::file_events::snapshot_revision(&state.peek());
    let fresh = api::fetch_file(&ctx.transport, &ctx.client, &ctx.session, &target.id).await;
    if !ctx.current(&state.peek()) {
        if matches!(&fresh, Ok(file) if policy::deleted(file))
            || matches!(&fresh, Err(Error::Api(code)) if code == "file_deleted")
        {
            recovery::resolve(state, &target, ui, pending.as_ref(), Outcome::Confirmed, 0).await;
        } else if let Err(error) = &fresh {
            recovery::cooldown(state, &target, ui, pending.as_ref(), error).await;
        }
        return;
    }
    if let Err(error) = &fresh
        && recovery::cooldown(state, &target, ui, pending.as_ref(), error).await
    {
        return;
    }
    let fresh = match fresh {
        Ok(file) if policy::deleted(&file) => {
            if recovery::resolve(state, &target, ui, pending.as_ref(), Outcome::Confirmed, 0).await
                && ctx.current(&state.peek())
            {
                removed(state, &target);
                if let Ok(mut current) = ui.try_write() {
                    *current = ActionState::default();
                }
            }
            return;
        }
        Ok(file) => file,
        Err(Error::Api(code)) if code == "file_deleted" => {
            if recovery::resolve(state, &target, ui, pending.as_ref(), Outcome::Confirmed, 0).await
                && ctx.current(&state.peek())
            {
                removed(state, &target);
                if let Ok(mut current) = ui.try_write() {
                    *current = ActionState::default();
                }
            }
            return;
        }
        Err(_) => {
            finish(
                ui,
                "This file could not be checked. It may be unavailable or you may no longer have access.",
                check_only,
            );
            return;
        }
    };
    if check_only {
        finish(
            ui,
            "The file is still available. This does not confirm whether the earlier deletion finished; check again later or review it in Slack.",
            true,
        );
        return;
    }
    if !policy::can_delete(&fresh, &ctx.session.user_id) {
        finish(
            ui,
            "You can only delete files you uploaded. Refresh Files to see current permissions.",
            false,
        );
        return;
    }
    if file_title(&fresh) != target.title {
        finish(
            ui,
            "This file was renamed. Refresh Files and confirm its new name before deleting.",
            false,
        );
        return;
    }
    let Some(started) = recovery::begin(
        state,
        &target,
        ui,
        Operation::Delete {
            original_title: target.title.clone(),
        },
    )
    .await
    else {
        return;
    };
    pending = Some(started);
    if !ctx.current(&state.peek())
        || crate::file_events::changed_since(&state.peek(), &target.team, &target.id, read_revision)
    {
        if !recovery::resolve(state, &target, ui, pending.as_ref(), Outcome::Rejected, 0).await {
            return;
        }
        if ctx.current(&state.peek()) {
            finish(
                ui,
                "This file changed while preparing deletion. No deletion was sent; review the latest file before trying again.",
                false,
            );
        }
        return;
    }
    let result = ctx
        .transport
        .execute(api::file_delete(&ctx.client, &ctx.session, &target.id))
        .await;
    match result {
        Ok(_) => {
            if recovery::resolve(state, &target, ui, pending.as_ref(), Outcome::Confirmed, 0).await
                && ctx.current(&state.peek())
            {
                removed(state, &target);
                if let Ok(mut current) = ui.try_write() {
                    *current = ActionState::default();
                }
            }
        }
        Err(Error::Api(code)) if code == "file_deleted" => {
            if recovery::resolve(state, &target, ui, pending.as_ref(), Outcome::Confirmed, 0).await
                && ctx.current(&state.peek())
            {
                removed(state, &target);
                if let Ok(mut current) = ui.try_write() {
                    *current = ActionState::default();
                }
            }
        }
        Err(error) => {
            let (message, uncertain) = match &error {
                Error::Api(code)
                    if matches!(
                        code.as_str(),
                        "cant_delete_file"
                            | "delete_not_allowed"
                            | "file_deleting_disabled"
                            | "missing_scope"
                            | "not_authed"
                            | "invalid_auth"
                            | "token_revoked"
                    ) =>
                {
                    ("Slack did not allow this file to be deleted.", false)
                }
                Error::TransportNotConfigured => ("Reconnect to Slack to delete this file.", false),
                Error::RateLimited { .. } => (
                    "Slack is limiting file changes. Wait a moment before trying again.",
                    false,
                ),
                _ => (
                    "Deletion could not be confirmed. Check its status before trying again.",
                    true,
                ),
            };
            if !uncertain {
                let retry_at = match &error {
                    Error::RateLimited { retry_after_secs } => {
                        recovery::now().saturating_add(retry_after_secs.unwrap_or(30))
                    }
                    _ => 0,
                };
                if !recovery::resolve(
                    state,
                    &target,
                    ui,
                    pending.as_ref(),
                    Outcome::Rejected,
                    retry_at,
                )
                .await
                    || !ctx.current(&state.peek())
                {
                    return;
                }
                if let Ok(mut current) = ui.try_write() {
                    current.retry_at = retry_at;
                }
            }
            if !ctx.current(&state.peek()) {
                return;
            }
            state
                .write()
                .report_failure(&error, "Reconnect to Slack to delete this file.", || {
                    message.into()
                });
            finish(ui, message, uncertain);
        }
    }
}

#[component]
pub fn Actions(
    state: Signal<ShellState>,
    target: Target,
    #[props(default)] in_viewer: bool,
    #[props(default)] recovery_list: bool,
) -> Element {
    let mut expanded = use_signal(|| false);
    let mut action = use_signal(ActionState::default);
    let mut load_serial = use_signal(|| 0_u64);
    let current = action.read().clone();
    let copy_target = target.clone();
    let open_target = target.clone();
    let delete_target = target.clone();
    let rename_target = target.clone();
    let save_target = target.clone();
    let load_target = target.clone();
    let retire_target = target.clone();
    let button_id = format!(
        "file-actions-{}-{}-{}-{}-{}",
        target.epoch, target.team, target.id, in_viewer, recovery_list
    );
    let focus_id = button_id.clone();
    let reveal_id = button_id.clone();
    let feedback_id = button_id.clone();
    let reveal = use_memo(move || {
        (
            expanded(),
            action.read().confirm,
            action.read().rename.is_some(),
            action.read().retire_confirm,
        )
    });
    use_effect(move || {
        let (open, _, _, _) = reveal();
        if !open {
            return;
        }
        let id = serde_json::to_string(&reveal_id).unwrap();
        document::eval(&format!(
            r#"requestAnimationFrame(()=>{{
            const row=document.getElementById({id})?.closest('.file-result,.viewer-footer');
            const panel=row?.querySelector('.file-action-panel'),list=row?.closest('.files-results,.viewer,.file-recovery-list');
            if(!panel||!list)return;
            const focus=panel.querySelector('input:not(:disabled)')||panel.querySelector('button:not(:disabled)');
            focus?.focus({{preventScroll:true}});
            if(focus?.tagName==='INPUT')focus.select();
            const r=panel.getBoundingClientRect(),v=list.getBoundingClientRect();
            if(r.bottom>v.bottom)list.scrollTop+=r.bottom-v.bottom+4;
            else if(r.top<v.top)list.scrollTop+=r.top-v.top-4;
            panel.scrollIntoView({{block:'nearest'}});
        }});"#
        ));
    });
    let feedback = use_memo(move || (expanded(), action.read().error.clone()));
    use_effect(move || {
        let (open, error) = feedback();
        if !open || error.is_none() {
            return;
        }
        let id = serde_json::to_string(&feedback_id).unwrap();
        document::eval(&format!(
            "requestAnimationFrame(()=>document.getElementById({id})?.closest('.file-result,.viewer-footer')?.querySelector('.file-action-error')?.scrollIntoView({{block:'nearest'}}));"
        ));
    });
    rsx! {
        div {class:"file-row-actions",onmounted:move |_|{dioxus::core::spawn_forever(rename_check::run(state,action));dioxus::core::spawn_forever(recovery_check::run(state));},
            button {id:button_id.clone(),class:"icon-btn file-more",title:"File actions", "aria-label":"Actions for {target.title}","aria-expanded":expanded(),
                disabled:current.busy,
                onclick:move |_|{
                    expanded.toggle();load_serial+=1;
                    let retry_at=action.peek().retry_at;
                    action.set(ActionState {busy:expanded(),retry_at,..Default::default()});
                    if expanded() {dioxus::core::spawn_forever(recovery::load(state,load_target.clone(),action,expanded,load_serial,*load_serial.peek()));}
                },
                {crate::icons::icon(crate::icons::Icon::More,"icon")}
            }
        }
        if expanded() {
            div {class:"file-action-panel",role:"group","aria-label":"Actions for {target.title}","data-escape-local":"true",
                onkeydown:move |event|{if event.key()==Key::Escape {event.stop_propagation();event.prevent_default();if !action.peek().busy {expanded.set(false);back(action);let id=serde_json::to_string(&focus_id).unwrap();document::eval(&format!("document.getElementById({id})?.focus();"));}}},
                if current.retire_confirm {
                    p {class:"file-delete-description","Stop checking this earlier change? This does not undo it or confirm its outcome in Slack. Review the file in Slack before making another change."}
                    div {class:"file-action-buttons",
                        button {disabled:current.busy,onclick:move |_|action.write().retire_confirm=false,"Keep checking"}
                        button {disabled:current.busy,onclick:move |_|{dioxus::core::spawn_forever(recovery::retire(state,retire_target.clone(),action));},"Stop tracking"}
                    }
                } else if let Some(edit)=current.rename.as_ref() {
                    form {class:"file-rename",onsubmit:move |event|{event.prevent_default();rename::start(state,save_target.clone(),action);},
                        label {r#for:"{button_id}-name","Name in Slack"}
                        input {id:"{button_id}-name",type:"text",value:edit.draft.clone(),disabled:current.busy||current.uncertain||current.blocked,
                            oninput:move |event|{let mut ui=action.write();if !ui.busy&&!ui.uncertain&&!ui.blocked {if let Some(edit)=&mut ui.rename {edit.draft=event.value();}ui.error=None;}}
                        }
                        p {class:"file-action-note","The download filename may also change."}
                        div {class:"file-action-buttons",
                            button {type:"button",disabled:current.busy,onclick:move |_|back(action),"Cancel"}
                            button {type:"submit",disabled:current.busy||current.blocked||(!current.uncertain&&(policy::rename_title(&edit.draft).is_err()||edit.draft.trim()==edit.original)),
                                if current.busy {"Checking…"}else if current.uncertain {"Check status"}else{"Save name"}
                            }
                        }
                    }
                } else if current.confirm {
                    p {class:"file-delete-description",
                        if current.uncertain {"Deletion of “{target.title}” is not confirmed. Check status to see whether Slack applied it."}
                        else {"Delete “{target.title}”? This permanently removes the file from Slack for everyone."}
                    }
                    div {class:"file-action-buttons",
                        button {disabled:current.busy,onclick:move |_|back(action),"Cancel"}
                        button {class:if current.uncertain {""} else {"file-delete"},disabled:current.busy,onclick:move |_|{
                            let check=action.peek().uncertain;
                            action.write().busy=true;action.write().error=None;
                            dioxus::core::spawn_forever(delete(state,delete_target.clone(),action,check));
                        },if current.busy {"Checking…"}else if current.uncertain {"Check status"}else{"Delete file"}}
                    }
                } else if current.busy {
                    p {class:"file-action-note",role:"status","Checking saved changes…"}
                } else {
                    if let Some(saved_target) = crate::later::file_actions::Target::capture(&state.read(), &target.id, &target.title) {
                        later::SavedControl { key:"{state.read().draft_writes.view_id}:{state.read().core.active_account:?}:{state.read().core.account_epoch}:{target.team}:{target.id}", state, target:saved_target }
                    }
                    div {class:"file-action-buttons",
                        button {disabled:target.link.is_none(),onclick:move |_|{dioxus::core::spawn_forever(copy(state,copy_target.clone()));},"Copy link"}
                        button {disabled:target.link.is_none(),onclick:move |_|open(state,&open_target),"Open in Slack"}
                        if target.owned {
                            if target.renameable {button {disabled:current.blocked,onclick:move |_|{let retry_at=action.peek().retry_at;action.set(ActionState {rename:Some(rename::Edit::new(&rename_target.title)),retry_at,..Default::default()});},"Rename…"}}
                            button {class:"file-delete",disabled:current.blocked,onclick:move |_|action.write().confirm=true,"Delete…"}
                        }
                    }
                    if target.link.is_none() {p {class:"file-action-note","This file has no available Slack link."}}
                }
                if current.uncertain && current.receipt.is_some() && !current.retire_confirm {
                    button {disabled:current.busy,onclick:move |_|action.write().retire_confirm=true,"Stop tracking…"}
                }
                if let Some(error)=current.error {p {class:"file-action-error",role:"alert","{error}"}}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewer_actions_follow_identity_and_current_workspace_ownership() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let team = shell.core.active_team.clone().unwrap();
        let uploader = shell.core.workspaces[&team].self_user_id.clone();
        let mut viewer = crate::state::ViewerVm {
            file: None,
            id: super_platinum_core::MediaAssetId::new(
                super_platinum_core::MediaAssetKind::Attachment,
            ),
            name: "Display title".into(),
            download_name: "original.txt".into(),
            mime: "text/plain".into(),
        };
        assert!(Target::for_viewer(&shell, &viewer).is_none());
        viewer.file = Some(policy::FileIdentity {
            id: "F1".into(),
            permalink: Some("https://team.slack.com/files/U1/F1".into()),
            uploader: Some(uploader),
            renameable: true,
        });
        let target = Target::for_viewer(&shell, &viewer).unwrap();
        assert!(target.owned);
        assert_eq!(target.title, "Display title");
        assert!(target.current(&shell));
        shell.core.account_epoch += 1;
        assert!(!target.current(&shell));
        viewer.file.as_mut().unwrap().uploader = Some("OTHER".into());
        assert!(!Target::for_viewer(&shell, &viewer).unwrap().owned);
        shell
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .deleted_files
            .insert("F1".into());
        assert!(Target::for_viewer(&shell, &viewer).is_none());
    }
}
