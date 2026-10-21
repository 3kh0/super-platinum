//! Only a confirmed self-removal can deliberately detach its admitted sequence.
//! Unrelated navigation never creates this authority, and a new read revokes it.
use super::*;
#[derive(Default)]
pub(in crate::app_home::about::management::permissions) struct Departure {
    pub armed: bool,
    pub departed: bool,
    pub revoked: bool,
    revision: Option<u64>,
    account: Option<String>,
}
impl Departure {
    pub(in crate::app_home::about::management::permissions) fn snapshot(&self) -> Value {
        json!({"armed":self.armed,"departed":self.departed,"revoked":self.revoked})
    }
}
pub(in crate::app_home::about::management) fn revoke(s: &mut ShellState, visit: u64) {
    for a in &mut s.app_home.about.management.permissions.attempts {
        if a.original.visit == visit && a.departure.departed {
            a.departure.revoked = true;
        }
    }
}
pub(in crate::app_home::about::management::permissions) fn dispatch_current(
    s: &ShellState,
    id: u64,
) -> bool {
    if exact(s, id) {
        return true;
    }
    let Some(a) = run::attempt(s, id) else {
        return false;
    };
    let d = &a.departure;
    d.armed
        && d.departed
        && !d.revoked
        && !a.finished
        && a.manager_group
            .as_ref()
            .is_some_and(|g| g.plan.self_removal())
        && admitted(s, a.original.visit)
        && a.original.owner.current(s)
        && a.original.policy.current(s)
        && s.core.active_account == d.account
        && s.app_home
            .about
            .management
            .surface
            .as_ref()
            .is_some_and(|h| {
                h.id == a.original.visit
                    && h.document.is_none()
                    && h.read.is_none()
                    && h.policy
                        .as_ref()
                        .is_some_and(|p| p.same(&a.original.policy))
            })
        && s.app_home.about.management.documents.iter().any(|doc| {
            doc.id == a.original.document
                && Some(doc.revision) == d.revision
                && doc.pending == Some(id)
                && doc.title == a.original.title
                && doc.description == a.original.description
                && a.original.editor.same_revision(&doc.editor)
        })
        && a.manager_source
            .as_ref()
            .is_some_and(|m| m.current(s, &a.original))
        && a.settings_source
            .as_ref()
            .is_none_or(|v| v.source_current(s, &a.original))
}
pub(in crate::app_home::about::management::permissions) async fn dispatch_native(
    state: Signal<ShellState>,
    id: u64,
    o: &Original,
) -> bool {
    if !dispatch_current(&state.peek(), id) {
        return false;
    }
    if run::attempt(&state.peek(), id).is_some_and(|a| a.departure.departed) {
        return dispatch_current(&state.peek(), id);
    }
    super::super::native::current_for(o, true).await && dispatch_current(&state.peek(), id)
}
fn failed(a: &Attempt) -> bool {
    let bad = |o: &Outcome| {
        matches!(
            o,
            Outcome::NotSent | Outcome::Rejected(_) | Outcome::Unknown
        )
    };
    a.copy_changed && a.plan.slots().iter().any(|s| bad(s.outcome()))
        || a.manager_group
            .as_ref()
            .is_some_and(|g| g.plan.slots().iter().any(|s| bad(s.outcome())))
        || a.run_group.as_ref().is_some_and(|g| {
            g.plan.slots().iter().any(|s| bad(s.outcome()))
                || g.followup.as_ref().is_some_and(|f| {
                    f.stages()
                        .iter()
                        .flat_map(|s| s.slots())
                        .any(|s| bad(s.outcome()))
                })
        })
}
fn all_succeeded(a: &Attempt) -> bool {
    (!a.copy_changed || a.plan.primary_succeeded())
        && a.run_group.as_ref().is_none_or(groups::Run::complete)
        && a.manager_group.as_ref().is_some_and(Group::complete)
}
pub(in crate::app_home::about::management::permissions) async fn maybe_depart(
    mut state: Signal<ShellState>,
    id: u64,
    o: &Original,
) {
    let eligible = |s: &ShellState| {
        run::attempt(s, id).is_some_and(|a| {
            a.departure.armed
                && !a.departure.departed
                && !a.departure.revoked
                && a.manager_group
                    .as_ref()
                    .is_some_and(|g| g.plan.self_removal())
                && (failed(a) || all_succeeded(a))
        }) && exact(s, id)
    };
    if !eligible(&state.peek()) || !super::super::native::current_for(o, true).await {
        return;
    }
    let Some(buffers) = super::super::native::finish_snapshot(o).await else {
        return;
    };
    if buffers.title != o.title || buffers.description != o.description {
        return;
    }
    let mut s = state.write();
    if !eligible(&s) {
        return;
    }
    let account = s.core.active_account.clone();
    let revision = {
        let Some(doc) = document_mut(&mut s, o.visit) else {
            return;
        };
        doc.title = buffers.title;
        doc.description = buffers.description;
        doc.notice = Some(
            "You left workflow management. Your draft and permission responses are retained."
                .into(),
        );
        doc.revision = next();
        doc.revision
    };
    let a = run::attempt_mut(&mut s, id).unwrap();
    a.departure.departed = true;
    a.departure.revision = Some(revision);
    a.departure.account = account;
    let release = a.release.take();
    let h = s.app_home.about.management.surface.as_mut().unwrap();
    h.document = None;
    h.read = None;
    h.notice=Some("You chose to leave workflow management. Permission responses and your local draft are retained.".into());
    // This is the existing Managed by you surface. No successor read or mutation is started.
    drop(s);
    if let Some(release) = release {
        release.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn departed() -> (ShellState, u64, u64) {
        let (mut s, visit, id) = super::super::super::tests::prepared();
        let o = run::attempt(&s, id).unwrap().original.clone();
        let source = Managers::capture(&s, &o).unwrap();
        let mut draft = Draft::new(&o.editor, &o.owner.ctx.session.user_id).unwrap();
        draft.select(vec![Some("UOTHER001".into())]);
        let revision = document(&s, visit).unwrap().revision;
        let account = s.core.active_account.clone();
        let a = run::attempt_mut(&mut s, id).unwrap();
        a.manager_group = Some(Group::new(draft.plan().unwrap()));
        a.manager_source = Some(source);
        a.departure = Departure {
            armed: true,
            departed: true,
            revoked: false,
            revision: Some(revision),
            account,
        };
        let h = s.app_home.about.management.surface.as_mut().unwrap();
        h.document = None;
        h.read = None;
        (s, visit, id)
    }
    #[test]
    fn explicit_departure_permits_only_original_sequence_and_fresh_read_revokes() {
        let (mut s, visit, id) = departed();
        assert!(dispatch_current(&s, id));
        assert!(!exact(&s, id));
        let lease = read::Lease::begin(&mut s, visit).unwrap();
        assert!(run::attempt(&s, id).unwrap().departure.revoked);
        lease.finish(&mut s, None);
        assert!(!dispatch_current(&s, id));
    }
    #[test]
    fn source_account_replacement_and_unrelated_document_never_inherit_departure() {
        let (mut s, _, id) = departed();
        s.core.active_account = Some("successor".into());
        assert!(!dispatch_current(&s, id));
        let (mut s, _, id) = departed();
        s.app_home
            .about
            .management
            .surface
            .as_mut()
            .unwrap()
            .document = Some(next());
        assert!(!dispatch_current(&s, id));
        let (mut s, _, id) = departed();
        let team = run::attempt(&s, id)
            .unwrap()
            .original
            .owner
            .ctx
            .session
            .team_id
            .clone();
        s.core.workspaces.get_mut(&team).unwrap().users.clear();
        assert!(!dispatch_current(&s, id));
    }
    #[test]
    fn token_cannot_be_fabricated_by_ordinary_navigation_or_finished_attempt() {
        let (mut s, _, id) = departed();
        run::attempt_mut(&mut s, id).unwrap().departure.armed = false;
        assert!(!dispatch_current(&s, id));
        let (mut s, _, id) = departed();
        run::attempt_mut(&mut s, id).unwrap().finished = true;
        assert!(!dispatch_current(&s, id));
        let (mut s, _, id) = departed();
        run::attempt_mut(&mut s, id).unwrap().manager_group = None;
        assert!(!dispatch_current(&s, id));
    }
}
