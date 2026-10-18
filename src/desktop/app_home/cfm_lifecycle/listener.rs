//! One process-local root task observes event occurrence, never key content.
//! Eval stays inside this task; it is never put into the Send shell state.
use std::{cell::RefCell, sync::Arc, time::Duration};

thread_local! {
    static OWNER: RefCell<Owner> = RefCell::new(Owner::default());
}
#[derive(Default)]
struct Owner {
    token: Option<Arc<()>>,
    ready: bool,
    changes: Option<tokio::sync::watch::Sender<bool>>,
}
pub(super) fn ready() -> bool {
    OWNER.with(|owner| owner.borrow().ready)
}
fn finish(token: &Arc<()>) {
    OWNER.with(|owner| {
        let mut owner = owner.borrow_mut();
        if owner
            .token
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, token))
        {
            owner.ready = false;
            owner.token = None;
            if let Some(changes) = owner.changes.take() {
                changes.send_replace(false);
            }
        }
    });
}
struct Guard {
    token: Arc<()>,
    eval: dioxus::document::Eval,
}
impl Drop for Guard {
    fn drop(&mut self) {
        // Admission closes immediately, but a new listener cannot be installed
        // until the old renderer's finally has actually removed its callback.
        OWNER.with(|owner| {
            let mut owner = owner.borrow_mut();
            if owner
                .token
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, &self.token))
            {
                owner.ready = false;
                if let Some(changes) = &owner.changes {
                    changes.send_replace(false);
                }
            }
        });
        if self.eval.send(()).is_err() {
            return; // Unknown cleanup is unavailable, never permission to duplicate.
        }
        let eval = self.eval;
        let token = self.token.clone();
        dioxus::core::spawn_forever(async move {
            if matches!(
                tokio::time::timeout(Duration::from_secs(5), eval.join::<()>()).await,
                Ok(Ok(()))
            ) {
                finish(&token);
            }
        });
    }
}
/// Called before native boot work. Failure leaves CFM admission unavailable;
/// it does not manufacture an unpaused observation or block ordinary startup.
pub(crate) async fn install() -> bool {
    if crate::fixture::is_fixture()
        && !super::super::channel_fetches::conditional::fixture::enabled()
        && !crate::directory_member_fetches::automatic::fixture_enabled()
    {
        return false;
    }
    let (token, mut changed) = OWNER.with(|owner| {
        let mut owner = owner.borrow_mut();
        if let Some(changes) = &owner.changes {
            return (None, changes.subscribe());
        }
        let token = Arc::new(());
        let (changes, receiver) = tokio::sync::watch::channel(false);
        owner.token = Some(token.clone());
        owner.changes = Some(changes);
        (Some(token), receiver)
    });
    if let Some(token) = token {
        dioxus::core::spawn_forever(run(token));
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if *changed.borrow_and_update() {
                return true;
            }
            if changed.changed().await.is_err() {
                return false;
            }
        }
    })
    .await
    .unwrap_or(false)
}
async fn run(token: Arc<()>) {
    let mut guard = Guard {
        token: token.clone(),
        eval: dioxus::document::eval(include_str!("listener.js")),
    };
    let reply = tokio::time::timeout(Duration::from_secs(5), guard.eval.recv::<String>()).await;
    if !matches!(reply, Ok(Ok(ref value)) if value == "ready") {
        return;
    }
    let accepted = OWNER.with(|owner| {
        let mut owner = owner.borrow_mut();
        if !owner
            .token
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &token))
        {
            return false;
        }
        owner.ready = true;
        if let Some(changes) = &owner.changes {
            changes.send_replace(true);
        }
        true
    });
    if !accepted {
        return;
    }
    while let Ok(event) = guard.eval.recv::<String>().await {
        if event != "keypress" || !ready() {
            break;
        }
        super::pause::keypress();
    }
}
