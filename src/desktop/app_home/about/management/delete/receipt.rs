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
    let book = &s.app_home.about.management.deletions;
    let confirmation = book.confirmation.as_ref().map(|c| {
        json!({"id":c.original.id,"visit":c.original.visit,
        "document":c.original.document,"revision":c.original.revision,"title":c.original.title,
        "description":c.original.description,"workflow":c.original.editor.workflow_id(),
        "current":c.original.current(s,None)})
    });
    let attempts:Vec<_>=book.attempts.iter().map(|a|{
        let slots:Vec<_>=a.plan.slots().iter().enumerate().map(|(i,slot)|json!({
            "index":i,"method":slot.plan().method(),"arguments":slot.plan().arguments(),
            "prerequisite":slot.prerequisite(),"outcome":phase(slot.outcome()),
            "raw":match &a.replies[i]{Some(Reply::Raw(raw))=>Some(raw),_=>None},
            "failure":match &a.replies[i]{Some(Reply::Failure(text))=>Some(*text),_=>None},
        })).collect();
        json!({"id":a.original.id,"visit":a.original.visit,"document":a.original.document,
            "revision":a.original.revision,"title":a.original.title,"description":a.original.description,
            "workflow":a.original.editor.workflow_id(),"finished":a.finished,"terminal":a.terminal,
            "noSend":a.no_send,"primarySucceeded":a.plan.primary_succeeded(),"current":exact(s,a.original.id),"slots":slots})
    }).collect();
    let draft = document(s, visit).map(|d| {
        json!({"id":d.id,"revision":d.revision,"title":d.title,
        "description":d.description,"pending":d.pending,"editor":d.editor.raw_workflow(),
        "trigger":d.editor.trigger(),"notice":d.notice})
    });
    json!({"visit":visit,"visible":visible(s,visit),"confirmation":confirmation,"document":draft,
        "terminal":terminal(s,visit),"attempts":attempts})
}
pub(in crate::app_home::about::management) fn outcomes(s: &ShellState, visit: u64) -> Vec<String> {
    let id = document(s, visit).and_then(|d| d.pending);
    let Some(a) = s
        .app_home
        .about
        .management
        .deletions
        .attempts
        .iter()
        .find(|a| Some(a.original.id) == id)
    else {
        return vec![];
    };
    a.plan
        .slots()
        .iter()
        .map(|slot| {
            format!(
                "{}: {}",
                if slot.plan().method() == "functions.workflows.delete" {
                    "Workflow"
                } else {
                    "Trigger"
                },
                match slot.outcome() {
                    Outcome::Pending | Outcome::NotSent => "Not sent",
                    Outcome::InFlight => "Waiting for response",
                    Outcome::Succeeded(_) => "Acknowledged",
                    Outcome::Rejected(_) => "Rejected",
                    Outcome::Unknown => "Unconfirmed",
                }
            )
        })
        .collect()
}
