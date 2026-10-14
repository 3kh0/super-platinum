//! Release the exact individual Activity read admission even if its view unmounts.
use crate::state::ShellState;
use dioxus::prelude::*;

pub(super) struct Reservation {
    pub state: Signal<ShellState>,
    pub id: u64,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut state = self.state;
        let id = self.id;
        match state.try_write() {
            Ok(mut shell) => {
                crate::read_shortcuts::bulk::end_individual(&mut shell, id);
                return;
            }
            Err(BorrowMutError::Dropped(_)) => return,
            Err(_) => {}
        }
        // Component drop can occur while the serial shell is borrowed. Retain
        // cleanup in the root scope; an old id never releases a newer request.
        dioxus::core::spawn_forever(async move {
            loop {
                tokio::task::yield_now().await;
                match state.try_write() {
                    Ok(mut shell) => {
                        crate::read_shortcuts::bulk::end_individual(&mut shell, id);
                        return;
                    }
                    Err(BorrowMutError::Dropped(_)) => return,
                    Err(_) => {}
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        });
    }
}
