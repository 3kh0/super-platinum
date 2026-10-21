use super::*;
pub(super) async fn run(mut state: Signal<ShellState>, id: u64) {
    if let Some(a) = attempt_mut(&mut state.write(), id) {
        a.phase = "publication";
    }
    for primary in [true, false] {
        if !authorized(state, id).await {
            return;
        }
        let plan = {
            let mut s = state.write();
            let Some(a) = attempt_mut(&mut s, id) else {
                return;
            };
            if primary {
                let Ok(editor) = working_editor(a) else {
                    a.local.notice=Some("Acknowledged workflow details could not be used for publication. Original responses are retained.".into());
                    return;
                };
                let Ok(plan) = MutationPlan::new(&editor, protocol::Command::Published(true))
                else {
                    return;
                };
                a.publication = plan;
                if a.publication.mark_sent(0).is_err() {
                    return;
                }
                a.publication.slots()[0].plan().clone()
            } else {
                let Some(f) = a.followup.as_mut() else { return };
                if f.phase() == protocol::FollowUpPhase::Complete {
                    return;
                }
                if f.mark_sent(0).is_err() {
                    return;
                }
                f.slots()[0].plan().clone()
            }
        };
        let (mut outcome, reply) = classify(
            send(
                state,
                id,
                &plan,
                if primary { "publish" } else { "trigger" },
            )
            .await,
        );
        let mut s = state.write();
        let Some(a) = attempt_mut(&mut s, id) else {
            return;
        };
        if primary {
            if let Outcome::Succeeded(raw) = &outcome {
                if !a
                    .publication
                    .original()
                    .publication()
                    .acknowledges_first_publish(raw)
                {
                    outcome = Outcome::Unknown;
                }
            }
            if a.publication.settle(0, outcome).is_err() {
                let _ = a.publication.settle(0, Outcome::Unknown);
            }
            a.publication_reply = Some(reply);
            if !a.publication.primary_succeeded() {
                return;
            }
            a.followup = protocol::FollowUpPlan::new(&a.publication).ok();
        } else {
            let f = a.followup.as_mut().unwrap();
            if f.settle(0, outcome).is_err() {
                let _ = f.settle(0, Outcome::Unknown);
            }
            a.follow_reply = Some(reply);
            let _ = f.advance();
        }
    }
}
