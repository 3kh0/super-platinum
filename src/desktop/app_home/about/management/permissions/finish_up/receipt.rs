use super::*;
fn phase(o: &Outcome) -> &'static str {
    match o {
        Outcome::Pending => "pending",
        Outcome::InFlight => "in-flight",
        Outcome::NotSent => "not-sent",
        Outcome::Succeeded(_) => "acknowledged",
        Outcome::Rejected(_) => "rejected",
        Outcome::Unknown => "unknown",
    }
}
fn slot(s: &protocol::Slot, r: Option<&Reply>) -> Value {
    json!({"method":s.plan().method(),"arguments":s.plan().arguments(),"outcome":phase(s.outcome()),"raw":match r{Some(Reply::Raw(v))=>Some(v),_=>None},"failure":match r{Some(Reply::Failure(v))=>Some(*v),_=>None}})
}
fn local(l: &Local) -> Value {
    json!({"title":l.title,"description":l.description,"iconUrl":l.icon_url,"showMore":l.more,"defaults":l.defaults,"caption":if l.defaults{"Next"}else{"Publish"},"epoch":l.epoch,"valid":l.valid(),"notice":l.notice,"avatar":l.avatar.value()})
}
pub(crate) fn snapshot(s: &ShellState, visit: u64) -> Value {
    let book = &s.app_home.about.management.permissions;
    let confirmation=book.confirmation.as_ref().filter(|c|c.original.embedded).map(|c|json!({"id":c.original.id,"visit":c.original.visit,"document":c.original.document,"revision":c.original.revision,"current":c.original.current(s,None),"local":local(c.finish_up.as_ref().unwrap()),"copy":c.selected,"runDraft":c.settings.as_ref().map(|v|v.snapshot()),"managerDraft":c.managers.as_ref().map(|v|v.snapshot())}));
    let attempts:Vec<_>=book.first_attempts.iter().map(|a|json!({
        "id":a.original.id,"visit":a.original.visit,"document":a.original.document,"revision":a.original.revision,"originalTitle":a.original.title,"originalDescription":a.original.description,
        "local":local(&a.local),"phase":a.phase,"continuation":a.continuation,"validation":a.validation,"finished":a.finished,"complete":a.complete,"resolved":a.resolved,"closed":a.closed,"current":exact(s,a.original.id),
        "metadata":a.metadata.as_ref().map(|p|p.slots().iter().enumerate().map(|(i,s)|slot(s,a.metadata_replies[i].as_ref())).collect::<Vec<_>>()),
        "copy":a.copy.as_ref().map(|p|slot(&p.slots()[0],a.copy_reply.as_ref())),"run":a.run.as_ref().map(groups::Run::snapshot),"managers":a.manager.as_ref().map(managers::Group::snapshot),
        "publication":slot(&a.publication.slots()[0],a.publication_reply.as_ref()),
        "followup":a.followup.as_ref().map(|p|json!({"phase":format!("{:?}",p.phase()),"fullySucceeded":p.fully_succeeded(),"stages":p.stages().iter().map(|stage|json!({"slots":stage.slots().iter().map(|s|slot(s,a.follow_reply.as_ref())).collect::<Vec<_>>()})).collect::<Vec<_>>()})),
        "readback":a.readback.as_ref().map(|(e,m)|json!({"workflow":e.raw_workflow(),"trigger":e.trigger(),"matches":m}))
        ,"confirmationAdd":a.confirmation_add.as_ref().map(|v|v.snapshot())
    })).collect();
    json!({"visit":visit,"visible":visible(s,visit),"confirmation":confirmation,"attempts":attempts,"avatarReceipts":book.first_avatar_receipts,
        "document":document(s,visit).map(|d|json!({"id":d.id,"revision":d.revision,"title":d.title,"description":d.description,"pending":d.pending,"editor":d.editor.raw_workflow(),"trigger":d.editor.trigger(),"notice":d.notice,"editable":editable(s,visit)}))})
}
pub(super) fn outcomes(a: &Attempt) -> Vec<String> {
    let mut rows = vec![];
    if let Some(p) = &a.metadata {
        for s in p.slots() {
            rows.push(format!(
                "{}: {}",
                if s.plan().method() == "functions.workflows.update" {
                    "Workflow details"
                } else {
                    "Trigger details"
                },
                phase(s.outcome())
            ));
        }
    }
    if let Some(p) = &a.copy {
        rows.push(format!("Copy access: {}", phase(p.slots()[0].outcome())));
    }
    if let Some(g) = &a.run {
        rows.push(format!(
            "Run access: {}",
            phase(g.plan.slots()[0].outcome())
        ));
    }
    if let Some(g) = &a.manager {
        for s in g.plan.slots() {
            rows.push(format!(
                "{}: {}",
                if s.plan().method().ends_with(".add") {
                    "Manager additions"
                } else {
                    "Manager removals"
                },
                phase(s.outcome())
            ));
        }
    }
    rows.push(format!(
        "Publication: {}",
        phase(a.publication.slots()[0].outcome())
    ));
    if let Some(p) = &a.followup {
        for s in p.stages().iter().flat_map(|v| v.slots()) {
            rows.push(format!("Trigger synchronization: {}", phase(s.outcome())));
        }
    }
    rows
}
