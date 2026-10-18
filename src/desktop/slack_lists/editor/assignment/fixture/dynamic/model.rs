use super::*;
fn person(p: &Recipient) -> Value {
    json!({"id":p.id,"label":p.label,"grant":p.grant.as_str(),"result":format!("{:?}",p.result),"dmAttempted":p.dm_attempted,"channel":p.channel})
}
pub(crate) fn diagnostic(shell: &ShellState) -> Result<Value, String> {
    let ledger = slot(|slot| {
        let s = slot
            .as_ref()
            .filter(|s| current(s, shell))
            .ok_or("Stale assignment fixture")?;
        if let Some(error) = &s.failure {
            return Err(error.clone());
        }
        Ok::<_, String>(
            json!({"requests":s.requests,"completed":s.completed,"visible":s.visible,
            "pending":s.pending.as_ref().map(|p|json!({"serial":p.serial,"draft":p.draft,"job":p.job,"method":p.method,"fields":p.fields})),
            "cells":s.cells,"cellCompletions":s.cell_completions}),
        )
    })?;
    let view = shell.slack_lists.view.as_ref().ok_or("Missing List")?;
    let loaded = view.loaded.as_ref().ok_or("Missing loaded page")?;
    let backend =
        crate::slack_lists::fixture::deletion_loaded(shell).ok_or("Missing canonical backend")?;
    let editor=crate::slack_lists::editor::presentation(shell).map(|e| {
        let original=match e.original.as_ref(){CellEditValue::Users(ids)=>json!(ids),_=>Value::Null};
        json!({"generation":e.generation,"row":e.row_id,"column":e.column_id,"kind":format!("{:?}",e.kind),
            "original":original,"disabled":e.disabled,"saving":e.saving,"stale":e.stale,"error":e.error,
            "query":e.directory.query,"loading":e.directory.loading,"choices":e.directory.candidates.iter().map(|v|json!({"id":v.id,"label":v.label})).collect::<Vec<_>>(),
            "review":e.assignment.map(|p|json!({"serial":p.serial,"stage":format!("{:?}",p.stage),"selected":p.selected,"message":p.message,"busy":p.busy,"canShare":p.can_share,"canNotNow":p.can_not_now,"canCheck":p.can_check,
                "people":p.people.iter().map(|v|json!({"id":v.id,"label":v.label,"grant":v.grant.as_str(),"status":v.status})).collect::<Vec<_>>()}))})
    });
    let entry=entry(shell).map(|e|json!({"serial":e.serial,"row":e.row,"column":e.column,"selected":e.selected,"stage":format!("{:?}",e.stage),"job":e.job,"message":e.message,
        "missing":e.intent.sharing_needed().ok(),"people":e.recipients.iter().map(person).collect::<Vec<_>>()}));
    let key = Key::current(shell).ok_or("Missing owner")?;
    let archive = registry(|r| {
        r.shares
            .iter()
            .filter(|p| p.key == key)
            .map(|p| json!({"serial":p.serial,"person":person(&p.person),"observed":p.observed}))
            .collect::<Vec<_>>()
    });
    let mutation=shell.slack_lists.operations.get(&view.id).map(|m|json!({"serial":m.attempt.serial,"phase":match m.phase {Phase::Preparing=>"Preparing",Phase::Saving=>"Saving",Phase::Uncertain=>"Uncertain"},"cells":m.attempt.edit.cells()}));
    Ok(
        json!({"document":loaded.document.raw,"population":loaded.records.raw,"backendDocument":backend.document.raw,"backend":backend.records.raw,
        "visit":view.generation,"revision":view.revision,"read":view.read,"selection":view.layout.selection_generation,"layout":view.layout.generation,
        "find":{"input":view.layout.find.input,"applied":view.layout.find.applied,"revision":view.layout.find.revision},
        "editor":editor,"entry":entry,"archive":archive,"mutation":mutation,"ledger":ledger,
        "recovery":recovery(shell).map(|r|json!({"serial":r.serial,"canAbandon":r.can_abandon,"inEditor":r.in_editor})),
        "noSession":shell.core.session.is_none(),"noTransport":shell.core.transport.is_none()}),
    )
}
