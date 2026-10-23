//! Original private reply entry and SQLite inventory, without rebinding.
use super::*;
pub(in crate::canvas) async fn snapshot(state: Signal<ShellState>) -> Result<Value, String> {
    use super::super::super::emoji_fixture as source;
    let saved = {
        let shell = state.peek();
        if !source::context_enabled(&shell) {
            return Err("Original Canvas emoji context retired".into());
        }
        shell.canvas_checklist.comment_replies.entries.iter().find(|e|e.disk.account==source::ACCOUNT&&e.root==source::ROOT).map(|e|(e.disk.clone(),e.root.clone(),json!({"id":e.id,"account":e.disk.account,"workspace":e.disk.scope.workspace,"user":e.disk.scope.user,"file":e.disk.scope.file,"root":e.root,"text":e.text,"blocks":e.blocks,"capture":e.capture,"serial":e.serial,"saving":e.saving,"loading":e.loading,"sending":e.sending,"error":e.error,"nativePresent":e.native.is_some()})))
    };
    let Some((disk, root, runtime)) = saved else {
        return Ok(Value::Null);
    };
    let scope = disk.scope.clone();
    let journal=disk.work(move|j|{let i=j.reply_inventory(&scope,&root)?;let h=j.reply_history(&scope,None)?;Ok(json!({"drafts":i.drafts(),"attempt":i.attempt(),"history":h.iter().map(|h|json!({"receipt":h.receipt,"status":h.status,"observed":h.observed})).collect::<Vec<_>>()}))}).await.map_err(|_|"Original reply journal read failed")?;
    Ok(json!({"runtime":runtime,"journal":journal,"ledger":super::ledger()}))
}
