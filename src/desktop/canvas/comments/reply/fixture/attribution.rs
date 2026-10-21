use super::*;
pub(in crate::canvas) async fn snapshot(state: Signal<ShellState>) -> Result<Value, String> {
    let saved = {
        let shell = state.peek();
        if !super::super::super::fixture::attribution::enabled(&shell)
            && !super::super::super::fixture::canonical_files::enabled(&shell)
        {
            return Err("Attribution fixture retired".into());
        }
        let e = shell
            .canvas_checklist
            .comment_replies
            .entries
            .iter()
            .find(|e| e.disk.account == ACCOUNT && e.root == TS);
        e.map(|e|(e.disk.clone(),e.root.clone(),json!({"text":e.text,"blocks":e.blocks,"capture":e.capture,"serial":e.serial,"saving":e.saving,"loading":e.loading,"sending":e.sending,"error":e.error,"nativePresent":e.native.is_some()})))
    };
    let Some((disk, root, runtime)) = saved else {
        return Ok(Value::Null);
    };
    let scope = disk.scope.clone();
    let journal=disk.work(move|j|{let i=j.reply_inventory(&scope,&root)?;let h=j.reply_history(&scope,None)?;
        Ok(json!({"drafts":i.drafts(),"attempt":i.attempt(),"history":h.iter().map(|h|json!({"receipt":h.receipt,"status":h.status,"observed":h.observed})).collect::<Vec<_>>()}))}).await.map_err(|_|"Read original reply journal failed")?;
    Ok(json!({"runtime":runtime,"journal":journal}))
}
