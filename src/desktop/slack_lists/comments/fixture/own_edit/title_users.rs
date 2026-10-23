//! Exact named title fixtures; one synthetic visit retirement, never an edit setter.
use super::*;
use crate::slack_lists::comments::own_edit;

pub(crate) fn named() -> bool {
    completion::title_users::own_edit_named() || completion::empty_canvas::own_edit_named()
}
pub(super) fn decorate(raw: &mut Value) {
    let leaves = raw["blocks"][0]["elements"][0]["elements"]
        .as_array_mut()
        .unwrap();
    leaves.extend([
        json!({"type":"text","text":" Canvas: "}),
        json!({"type":"canvas","file_id":completion::files::CANVAS,"text":"https://fixture.invalid/files/FLISTFILE1","url":"https://fixture.invalid/files/FLISTFILE1","style":{"bold":true},"opaque_title_occurrence":{"index":0}}),
        json!({"type":"text","text":" / "}),
        json!({"type":"canvas","file_id":completion::files::CANVAS,"text":"https://fixture.invalid/files/FLISTFILE1","url":"https://fixture.invalid/files/FLISTFILE1","style":{"italic":true},"opaque_title_occurrence":{"index":1}}),
    ]);
}
/// The native runner may replace only this visit, after exact durable custody.
/// The real owner guard and retire path retain the actual detached native holder.
pub(crate) fn replace_visit(shell: &mut ShellState) -> Result<(Value, own_edit::Binding), String> {
    if !named()
        || !(completion::title_users::case() == "own-edit-title-held-cancel"
            || (completion::empty_canvas::own_edit_named()
                && completion::title_users::case() == "own-edit-empty-recovery"))
        || !enabled(shell)
    {
        return Err("Exact closed title retirement case required".into());
    }
    let before = own_edit::snapshot(shell);
    let view = own_edit::presentation(shell).ok_or("Original edit missing")?;
    if !view.binding.current(shell) || !view.can_save || view.busy {
        return Err("Original current durable edit required".into());
    }
    let account = shell.core.active_account.clone();
    let epoch = shell.core.account_epoch;
    let session = session(shell).map_err(|e| e.to_string())?;
    let old = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("List visit missing")?
        .generation;
    let next = crate::slack_lists::next_id();
    shell.slack_lists.view.as_mut().unwrap().generation = next;
    own_edit::retire(shell);
    Ok((
        json!({"kind":"synthetic-list-visit-replacement","beforeVisit":old,"afterVisit":next,
        "account":account,"epoch":epoch,"team":session.team_id,"user":session.user_id,
        "before":before,"after":own_edit::snapshot(shell),"oldCurrent":view.binding.current(shell),
        "oldVisible":view.binding.visible_mount(shell),"oldRetained":view.binding.retained_mount(shell)}),
        view.binding,
    ))
}
