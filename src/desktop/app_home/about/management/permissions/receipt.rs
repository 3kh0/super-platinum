use super::*;
fn phase(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Pending => "pending",
        Outcome::InFlight => "in-flight",
        Outcome::NotSent => "not-sent",
        Outcome::Succeeded(_) => "acknowledged",
        Outcome::Rejected(_) => "rejected",
        Outcome::Unknown => "unknown",
    }
}
pub(crate) fn snapshot(s: &ShellState, visit: u64) -> Value {
    let book = &s.app_home.about.management.permissions;
    let confirmation=book.confirmation.as_ref().map(|c|json!({"id":c.original.id,"visit":c.original.visit,"document":c.original.document,"revision":c.original.revision,"title":c.original.title,"description":c.original.description,"workflow":c.original.editor.workflow_id(),"selected":c.selected,"originalCopy":c.original.editor.copy_permissions(),"runDraft":c.settings.as_ref().map(|v|v.snapshot()),"managerDraft":c.managers.as_ref().map(|v|v.snapshot()),"current":c.original.current(s,None)}));
    let attempts:Vec<_>=book.attempts.iter().map(|a|{
        let slots:Vec<_>=a.plan.slots().iter().enumerate().map(|(i,slot)|json!({"index":i,"method":slot.plan().method(),"arguments":slot.plan().arguments(),"prerequisite":slot.prerequisite(),"outcome":phase(slot.outcome()),"raw":match a.replies[i].as_ref(){Some(Reply::Raw(v))=>Some(v),_=>None},"failure":match a.replies[i].as_ref(){Some(Reply::Failure(v))=>Some(*v),_=>None}})).collect();
        json!({"id":a.original.id,"visit":a.original.visit,"document":a.original.document,"revision":a.original.revision,"title":a.original.title,"description":a.original.description,"workflow":a.original.editor.workflow_id(),"desired":a.desired,"copyChanged":a.copy_changed,"runGroup":a.run_group.as_ref().map(groups::Run::snapshot),"managerGroup":a.manager_group.as_ref().map(managers::Group::snapshot),"departure":a.departure.snapshot(),"originalCopy":a.original.editor.copy_permissions(),"finished":a.finished,"complete":a.complete,"resolved":a.resolved,"noSend":a.no_send,"primarySucceeded":a.plan.primary_succeeded(),"current":exact(s,a.original.id),"slots":slots,"readback":a.readback.as_ref().map(|(e,m)|json!({"workflow":e.raw_workflow(),"trigger":e.trigger(),"copy":e.copy_permissions(),"run":e.run_permissions(),"external":e.external_preferences(),"matches":m}))})
    }).collect();
    let draft=document(s,visit).map(|d|json!({"id":d.id,"revision":d.revision,"title":d.title,"description":d.description,"pending":d.pending,"editor":d.editor.raw_workflow(),"trigger":d.editor.trigger(),"copy":d.editor.copy_permissions(),"run":d.editor.run_permissions(),"external":d.editor.external_preferences(),"notice":d.notice,"editable":editable(s,visit)}));
    json!({"visit":visit,"visible":visible(s,visit),"confirmation":confirmation,"document":draft,"attempts":attempts})
}
pub(in crate::app_home::about::management) fn outcomes(s: &ShellState, visit: u64) -> Vec<String> {
    let Some(a) = document(s, visit)
        .and_then(|d| d.pending)
        .and_then(|id| run::attempt(s, id))
    else {
        return vec![];
    };
    let label = |o: &Outcome| match o {
        Outcome::Pending | Outcome::NotSent => "Not sent",
        Outcome::InFlight => "Waiting for response",
        Outcome::Succeeded(_) => "Acknowledged",
        Outcome::Rejected(_) => "Rejected",
        Outcome::Unknown => "Unconfirmed",
    };
    let mut rows = vec![];
    if a.copy_changed {
        rows.push(format!(
            "Copy access: {}",
            label(a.plan.slots()[0].outcome())
        ));
    }
    if let Some(g) = &a.manager_group {
        for slot in g.plan.slots() {
            rows.push(format!(
                "Workflow managers ({}): {}",
                if slot.plan().method().ends_with(".add") {
                    "add"
                } else {
                    "remove"
                },
                label(slot.outcome())
            ));
        }
    }
    if let Some(g) = &a.run_group {
        rows.push(format!(
            "Run access: {}",
            label(g.plan.slots()[0].outcome())
        ));
        if let Some(f) = &g.followup {
            for slot in f.stages().iter().flat_map(|s| s.slots()) {
                rows.push(format!("Workflow restrictions: {}", label(slot.outcome())));
            }
        }
    }
    rows
}
