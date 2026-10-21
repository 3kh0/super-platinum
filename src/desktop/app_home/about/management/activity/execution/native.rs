//! One evaluator retains original renderer identities across unlocked I/O.
use dioxus::document::{Eval, eval};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc, time::Duration};

struct Session {
    eval: Eval,
    lifetime: crate::app_home::inputs::native::Release,
    serial: Cell<u64>,
    busy: Cell<bool>,
    closed: Cell<bool>,
}
#[derive(Clone)]
pub(in crate::app_home::about::management::activity) struct Guard(Rc<Session>);
pub(in crate::app_home::about::management::activity) struct Release {
    session: Rc<Session>,
    serial: u64,
    released: Cell<bool>,
}
impl Release {
    /// Terminal cleanup acknowledgment. This does not grant read/publication
    /// authority: a successor must still capture the original native inputs.
    pub(in crate::app_home::about::management::activity) async fn finish(
        self,
        focus: bool,
    ) -> bool {
        if self.released.replace(true) {
            return false;
        }
        let guard = Guard(self.session.clone());
        let ok = guard
            .command("close", None, Some(self.serial), None, focus)
            .await
            .is_some();
        self.session.closed.set(true);
        self.session.lifetime.release();
        ok
    }
    pub(in crate::app_home::about::management::activity) fn release(&self) {
        if !self.released.replace(true) {
            let _ = self
                .session
                .eval
                .send(json!({"kind":"unlock","serial":self.serial}));
        }
    }
}
impl Drop for Release {
    fn drop(&mut self) {
        self.release();
    }
}
#[derive(Deserialize)]
struct Ack {
    serial: u64,
    ok: bool,
}
struct Receiving {
    session: Rc<Session>,
    completed: bool,
}
impl Drop for Receiving {
    fn drop(&mut self) {
        self.session.busy.set(false);
        if !self.completed {
            self.session.closed.set(true);
            self.session.lifetime.release();
        }
    }
}
impl Guard {
    async fn receive(&self, serial: u64) -> bool {
        let mut receiving = Receiving {
            session: self.0.clone(),
            completed: false,
        };
        let mut eval = self.0.eval;
        let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Ack>()).await;
        if let Ok(Ok(ack)) = result
            && ack.serial == serial
        {
            receiving.completed = true;
            return ack.ok;
        }
        false
    }
    async fn command(
        &self,
        kind: &str,
        read: Option<u64>,
        lease: Option<u64>,
        detail: Option<u64>,
        focus: bool,
    ) -> Option<u64> {
        if self.0.closed.get() || self.0.busy.replace(true) {
            return None;
        }
        let Some(serial) = self.0.serial.get().checked_add(1) else {
            self.0.closed.set(true);
            self.0.busy.set(false);
            self.0.lifetime.release();
            return None;
        };
        self.0.serial.set(serial);
        if self
            .0
            .eval
            .send(json!({"kind":kind,"serial":serial,"read":read,"lease":lease,"detail":detail.map(|id|id.to_string()),"focus":focus}))
            .is_err()
        {
            self.0.closed.set(true);
            self.0.busy.set(false);
            self.0.lifetime.release();
            return None;
        }
        self.receive(serial).await.then_some(serial)
    }
    pub(in crate::app_home::about::management::activity) async fn activate(
        &self,
        read: u64,
        detail: u64,
    ) -> bool {
        read != 0
            && self
                .command("activate", Some(read), None, Some(detail), false)
                .await
                .is_some()
    }
    pub(in crate::app_home::about::management::activity) async fn lock(&self) -> Option<Release> {
        let serial = self.command("lock", None, None, None, false).await?;
        Some(Release {
            session: self.0.clone(),
            serial,
            released: Cell::new(false),
        })
    }
}
pub(in crate::app_home::about::management::activity) async fn capture(
    expected: &Value,
) -> Option<(Guard, Release)> {
    let eval = eval(&format!(
        "{}\n{}\n{}\nawait workflowActivityExecution({expected},dioxus);",
        include_str!("../date_witness.js"),
        include_str!("../native.js"),
        include_str!("native.js")
    ));
    let guard = Guard(Rc::new(Session {
        eval,
        lifetime: crate::app_home::inputs::native::retain(eval),
        serial: Cell::new(0),
        busy: Cell::new(true),
        closed: Cell::new(false),
    }));
    if !guard.receive(0).await {
        return None;
    }
    let release = Release {
        session: guard.0.clone(),
        serial: 0,
        released: Cell::new(false),
    };
    Some((guard, release))
}
