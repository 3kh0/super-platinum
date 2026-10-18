//! Account reload replaces ShellState. Carry private custody and the exact
//! outstanding task/queued write identities; never carry presentation authority.
use super::*;

pub(crate) fn carry_pending(previous: &mut ShellState, loaded: &mut ShellState) {
    let mut entries = std::mem::take(&mut previous.profile_edit.recording.entries);
    for entry in entries.values_mut() {
        entry.checked = None;
        if let Some(receipt) = entry.receipt.as_mut() {
            receipt.stage = Stage::Unknown;
        }
    }
    loaded.profile_edit.recording.entries.append(&mut entries);
    loaded
        .profile_edit
        .recording
        .preferred
        .append(&mut previous.profile_edit.recording.preferred);
    loaded.profile_edit.recording.pane = None;
    previous.profile_edit.recording.pane = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    fn harness() -> (VirtualDom, Signal<ShellState>) {
        let slot = Rc::new(Cell::new(None));
        let mut dom = VirtualDom::new_with_props(
            |slot: Rc<Cell<Option<Signal<ShellState>>>>| {
                let state = use_signal(|| {
                    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
                    fixture::install(&mut shell);
                    shell
                });
                slot.set(Some(state));
                rsx! {}
            },
            slot.clone(),
        );
        dom.rebuild_in_place();
        (dom, slot.get().unwrap())
    }
    #[test]
    fn actual_account_install_keeps_late_ticket_without_restoring_request_authority() {
        let (dom, mut state) = harness();
        dom.in_runtime(|| {
            let r={
                let mut shell=state.write();
                let p=shell.profile_edit.recording.pane.clone().unwrap();
                shell.profile_edit.recording.pane.as_mut().unwrap().capture_phase="preview".into();
                let r=begin(&mut shell,p.visit,p.serial,Operation::Upload).unwrap();
                let scope=protocol::custody::Scope::new(&r.token.owner.account,&r.token.owner.team,&r.token.owner.user).unwrap();
                let mut record=protocol::custody::Record::capture(scope,&r.form).unwrap();
                record.set_audio(protocol::custody::Audio::new(vec![1,2,3],"audio/webm",1000).unwrap()).unwrap();
                record.begin(protocol::custody::Operation::Upload,1).unwrap();
                record.mark(protocol::custody::Stage::Ticket).unwrap();
                let e=entry_mut(&mut shell,&r).unwrap();e.retained=Some(record);e.inflight=Some(r.id);
                e.receipt.as_mut().unwrap().stage=Stage::Ticket;
                r
            };
            let loaded=ShellState::opening(crate::media::MediaRegistry::default());
            crate::bootstrap::install_loaded(state,loaded);
            let mut shell=state.write();
            assert!(presentation(&shell).is_none());
            assert!(!current(&shell,&r.token,r.visit,r.serial));
            let e=entry_mut(&mut shell,&r).expect("late response retains exact original receipt after account removal");
            let ticket=protocol::UploadTicket::decode(&serde_json::json!({"ok":true,"file":"F00000002","upload_url":"https://files.slack.com/upload/synthetic"})).unwrap();
            e.receipt.as_mut().unwrap().ticket=Some(ticket.clone());
            e.retained.as_mut().unwrap().remember_ticket(ticket.file()).unwrap();
            e.retained.as_mut().unwrap().mark(protocol::custody::Stage::Bytes).unwrap();
            storage::finish_task_shell(&mut shell,&r.token.owner,r.id);
            assert_eq!(shell.profile_edit.recording.entries.get(&r.token.owner).unwrap().retained.as_ref().unwrap().staged_file(),Some("F00000002"));
            assert!(presentation(&shell).is_none());
        });
    }
    #[test]
    fn actual_account_install_carries_queued_local_mutex_and_token_only() {
        let (dom, mut state) = harness();
        dom.in_runtime(|| {
            let (p, queued) = {
                let mut shell = state.write();
                let p = shell.profile_edit.recording.pane.clone().unwrap();
                let scope = protocol::custody::Scope::new(
                    &p.token.owner.account,
                    &p.token.owner.team,
                    &p.token.owner.user,
                )
                .unwrap();
                let mut record =
                    protocol::custody::Record::capture(scope, p.form.as_ref().unwrap()).unwrap();
                record
                    .set_audio(
                        protocol::custody::Audio::new(vec![4, 5, 6], "audio/webm", 1000).unwrap(),
                    )
                    .unwrap();
                let queued = queue::reserve(&mut shell, &p.token, record).unwrap();
                (p, queued)
            };
            crate::bootstrap::install_loaded(
                state,
                ShellState::opening(crate::media::MediaRegistry::default()),
            );
            let shell = state.peek();
            let e = shell
                .profile_edit
                .recording
                .entries
                .get(&p.token.owner)
                .unwrap();
            assert!(queue::owns(e, &p.token));
            assert!(e.io_gate.try_lock().is_err());
            assert_eq!(
                e.retained.as_ref().unwrap().audio().unwrap().bytes(),
                [4, 5, 6]
            );
            assert!(presentation(&shell).is_none());
            assert!(!p.token.current(&shell));
            drop(shell);
            drop(queued);
        });
    }
}
