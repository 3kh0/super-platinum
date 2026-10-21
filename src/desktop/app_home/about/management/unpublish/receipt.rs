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
fn slot(index: usize, s: &protocol::Slot, reply: Option<&Reply>) -> Value {
    json!({"index":index,"method":s.plan().method(),"arguments":s.plan().arguments(),"prerequisite":s.prerequisite(),"outcome":phase(s.outcome()),
        "raw":match reply{Some(Reply::Raw(v))=>Some(v),_=>None},"failure":match reply{Some(Reply::Failure(v))=>Some(*v),_=>None}})
}
pub(crate) fn snapshot(s: &ShellState, visit: u64) -> Value {
    let book = &s.app_home.about.management.unpublishing;
    let confirmation=book.confirmation.as_ref().map(|c|json!({"id":c.original.id,"visit":c.original.visit,"document":c.original.document,"revision":c.original.revision,"title":c.original.title,"description":c.original.description,"workflow":c.original.editor.workflow_id(),"current":c.original.current(s,None)}));
    let attempts:Vec<_>=book.attempts.iter().map(|a|{
        let slots:Vec<_>=a.plan.slots().iter().enumerate().map(|(i,s)|slot(i,s,a.replies[i].as_ref())).collect();
        let followup=a.followup.as_ref().map(|p|{
            let stages:Vec<_>=p.stages().iter().enumerate().map(|(i,stage)|{
                let slots:Vec<_>=stage.slots().iter().enumerate().map(|(j,s)|slot(j,s,a.follow_replies.get(i).and_then(|r|r.get(j)).and_then(Option::as_ref))).collect();
                json!({"phase":format!("{:?}",stage.phase()),"slots":slots})
            }).collect();
            json!({"phase":format!("{:?}",p.phase()),"fullySucceeded":p.fully_succeeded(),"workflow":p.current_workflow(),"trigger":p.current_trigger(),"stages":stages})
        });
        json!({"id":a.original.id,"visit":a.original.visit,"document":a.original.document,"revision":a.original.revision,"title":a.original.title,"description":a.original.description,"workflow":a.original.editor.workflow_id(),"finished":a.finished,"complete":a.complete,"resolved":a.resolved,"noSend":a.no_send,"primarySucceeded":a.plan.primary_succeeded(),"current":exact(s,a.original.id),"slots":slots,"followup":followup,
            "readback":a.readback.as_ref().map(|(e,m)|json!({"workflow":e.raw_workflow(),"trigger":e.trigger(),"matches":m}))})
    }).collect();
    let draft=document(s,visit).map(|d|json!({"id":d.id,"revision":d.revision,"title":d.title,"description":d.description,"pending":d.pending,"editor":d.editor.raw_workflow(),"trigger":d.editor.trigger(),"notice":d.notice,"editable":editable(s,visit)}));
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
    let mut result = vec![format!("Unpublish: {}", label(a.plan.slots()[0].outcome()))];
    if let Some(p) = &a.followup {
        let slots: Vec<_> = p.stages().iter().flat_map(|s| s.slots()).collect();
        if slots.is_empty() {
            result.push("Trigger synchronization: No change required".into());
        } else {
            result.extend(
                slots
                    .iter()
                    .map(|s| format!("Trigger synchronization: {}", label(s.outcome()))),
            );
        }
    }
    result
}
