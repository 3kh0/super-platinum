use super::*;
pub(super) fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s, false, false);
    let h = s.app_home.about.surface.as_ref().unwrap();
    let surface = Surface {
        id: next(),
        about: Some(Key {
            id: h.id,
            revision: h.revision,
        }),
        entry: None,
        owner: h.owner.clone().into(),
        account: s.core.active_account.clone(),
        query: ListQuery::default(),
        list: List::default(),
        document: None,
        read: None,
        policy: None,
        notice: None,
        fixture: true,
    };
    s.app_home.about.management.surface = Some(surface);
    let policy = policy::fixture_snapshot(&mut s);
    s.app_home.about.management.surface.as_mut().unwrap().policy = Some(policy);
    s
}
pub(super) fn editor(s: &mut ShellState) -> Editor {
    let owner = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .unwrap()
        .owner
        .clone();
    let mut load = EditorLoad::new("WF_REVIEW").unwrap();
    while let Some(r) = load.requests().into_iter().next() {
        let raw =
            fixture::response(s, &r.plan.request(&owner.ctx.client, &owner.ctx.session)).unwrap();
        load.accept(r.kind, raw).unwrap();
    }
    load.editor().unwrap()
}
#[test]
fn configuration_uses_actual_owner_ids_and_metadata_requires_observed_facts() {
    let mut s = shell();
    let h = s.app_home.about.surface.as_ref().unwrap();
    let key = Key {
        id: h.id,
        revision: h.revision,
    };
    assert!(matches!(
        capture(&s, key),
        Some(WorkflowEntry::Editor { .. })
    ));
    let current = editor(&mut s);
    assert!(allowed_editor(&current, "UWORKFLOW"));
    assert!(!allowed_editor(&current, "UOTHER"));
    fixture::set_mode(&mut s, fixture::Mode::MissingAvailability);
    assert!(!allowed_editor(&editor(&mut s), "UWORKFLOW"));
    fixture::install(&mut s, true, false);
    let h = s.app_home.about.surface.as_ref().unwrap();
    assert!(matches!(
        capture(
            &s,
            Key {
                id: h.id,
                revision: h.revision
            }
        ),
        Some(WorkflowEntry::Managed)
    ));
}
#[test]
fn exact_editor_unknown_fields_change_preflight_without_touching_local_text() {
    let mut s = shell();
    let before = editor(&mut s);
    fixture::set_mode(&mut s, fixture::Mode::ChangedPreflight);
    let after = editor(&mut s);
    assert!(!before.same_revision(&after));
    let title = "Révision de projet";
    let description = "  Exact description\nsecond line\n";
    let command = before.metadata_command(title, description, None).unwrap();
    let plan = MutationPlan::new(&before, command).unwrap();
    assert_eq!(plan.slots().len(), 2);
    assert!(plan.follow_ups().is_empty());
    assert_eq!(plan.slots()[0].plan().arguments()["title"], title);
    assert_eq!(
        plan.slots()[0].plan().arguments()["description"],
        description
    );
}
#[test]
fn fixture_partial_and_lost_replies_keep_independent_slots_and_no_observation_settlement() {
    for mode in [fixture::Mode::TriggerRejected, fixture::Mode::ReplyLoss] {
        let mut s = shell();
        let before = editor(&mut s);
        let mut plan = MutationPlan::new(
            &before,
            before
                .metadata_command("Updated workflow", "Exact update", None)
                .unwrap(),
        )
        .unwrap();
        fixture::set_mode(&mut s, mode);
        let owner = s
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .owner
            .clone();
        for i in 0..plan.slots().len() {
            plan.mark_sent(i).unwrap();
            let response = fixture::mutation(
                &mut s,
                &plan.slots()[i]
                    .plan()
                    .request(&owner.ctx.client, &owner.ctx.session),
            );
            let outcome = match response {
                Ok(raw) if raw["ok"] == true => Outcome::Succeeded(raw),
                Ok(raw) => Outcome::Rejected(raw),
                Err(_) => Outcome::Unknown,
            };
            plan.settle(i, outcome).unwrap();
        }
        let after = editor(&mut s);
        let slots = plan.slots().to_vec();
        assert_eq!(
            plan.observe(&after),
            if mode == fixture::Mode::ReplyLoss {
                Observation::Matches
            } else {
                Observation::Different
            }
        );
        assert_eq!(plan.slots(), slots);
        assert!(!plan.primary_succeeded());
        assert_eq!(fixture::snapshot(&s)["writes"], 2);
    }
}
