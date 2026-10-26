//! Offline observation only: never modifies the expected native lease snapshot.
use super::*;
use crate::app_home::inputs::external::fixture::capture;
use dioxus::desktop::DesktopContext;
mod gestures;
mod lanes;
pub(crate) use lanes::run;
#[derive(Default)]
pub(super) struct State {
    armed: bool,
    waiting: bool,
    gate: Option<oneshot::Sender<()>>,
    attempts: Vec<Value>,
    busy: Vec<Value>,
}
pub(crate) fn named() -> bool {
    super::named() && case() == "admission"
}
pub(crate) fn observation_active() -> bool {
    named()
        || crate::app_home::inputs::number_fixture::named()
        || crate::app_home::inputs::email_fixture::named()
        || crate::app_home::inputs::focus_fixture::named()
        || (crate::app_home::inputs::native::datetime::editing_fixture::named()
            && crate::app_home::inputs::native::datetime::editing_fixture::case() == "admission")
        || (crate::app_home::inputs::native::datetime::fixture::named()
            && crate::app_home::inputs::native::datetime::fixture::case() == "admission")
}
fn source(s: &ShellState) -> Value {
    let x = crate::app_home::inputs::external::fixture::snapshot(s);
    json!({"authority":x["authority"],"rawView":x["rawView"]})
}
pub(crate) async fn before_calendar_check(
    mut state: Signal<ShellState>,
    visit: actions::Key,
    expected: Value,
) -> Option<usize> {
    if !observation_active() {
        return None;
    }
    let started = Instant::now();
    let (index, before, rx) = {
        let mut s = state.write();
        let before = source(&s);
        let f = &mut s.app_home.text.fixture.as_mut()?.admission;
        if f.attempts.len() >= 24 {
            return None;
        }
        let index = f.attempts.len();
        let armed = std::mem::take(&mut f.armed);
        f.attempts.push(json!({"index":index,"visit":{"visit":visit.visit,"revision":visit.revision},"expected":expected,"sourceBefore":before,"held":armed,"sourceAfter":null,"sourceCurrent":null,"pausedMs":null,"accepted":null,"timedOut":false}));
        let rx = if armed {
            let (tx, rx) = oneshot::channel();
            f.waiting = true;
            f.gate = Some(tx);
            Some(rx)
        } else {
            None
        };
        (index, before, rx)
    };
    let timed_out = if let Some(rx) = rx {
        !matches!(
            tokio::time::timeout(Duration::from_secs(8), rx).await,
            Ok(Ok(()))
        )
    } else {
        false
    };
    let mut s = state.write();
    let after = source(&s);
    let current = before == after
        && s.app_home.surface.as_ref().is_some_and(|h| {
            h.owner.current(&s) && h.id == visit.visit && h.revision == visit.revision
        });
    let f = &mut s.app_home.text.fixture.as_mut()?.admission;
    f.waiting = false;
    let row = f.attempts.get_mut(index)?;
    row["sourceAfter"] = after;
    row["sourceCurrent"] = json!(current);
    row["pausedMs"] = json!(started.elapsed().as_millis());
    row["timedOut"] = json!(timed_out);
    Some(index)
}
pub(crate) fn calendar_result(mut state: Signal<ShellState>, index: Option<usize>, accepted: bool) {
    if !observation_active() {
        return;
    }
    if let Some(index) = index {
        if let Some(f) = state.write().app_home.text.fixture.as_mut() {
            if let Some(row) = f.admission.attempts.get_mut(index) {
                row["accepted"] = json!(accepted);
            }
        }
    }
}
pub(crate) fn busy_result(mut state: Signal<ShellState>, visit: actions::Key, busy: bool) {
    if !observation_active() {
        return;
    }
    let mut s = state.write();
    let observed = source(&s);
    if let Some(f) = s.app_home.text.fixture.as_mut() {
        if f.admission.busy.len() < 24 {
            f.admission.busy.push(json!({"attempt":f.admission.attempts.len().checked_sub(1),"visit":{"visit":visit.visit,"revision":visit.revision},"busy":busy,"source":observed}));
        }
    }
}
pub(crate) fn augment_observation(s: &ShellState, out: &mut Value) {
    if !observation_active() {
        return;
    }
    if let Some(f) = s.app_home.text.fixture.as_ref() {
        out["admission"] = json!({"waiting":f.admission.waiting,"attempts":f.admission.attempts,"busy":f.admission.busy});
    }
}
pub(super) fn augment(s: &ShellState, out: &mut Value) {
    if !named() {
        return;
    }
    augment_observation(s, out);
    let native = crate::app_home::inputs::native::date::proof(s);
    out["dateWidgets"] = native["dateWidgets"].clone();
    out["nativeMounts"] = native["nativeMounts"].clone();
}
pub(crate) fn arm(s: &mut ShellState) -> usize {
    let f = &mut s.app_home.text.fixture.as_mut().unwrap().admission;
    assert!(!f.armed && !f.waiting && f.gate.is_none());
    f.armed = true;
    f.attempts.len()
}
pub(crate) fn release(s: &mut ShellState) -> Result<(), String> {
    s.app_home
        .text
        .fixture
        .as_mut()
        .unwrap()
        .admission
        .gate
        .take()
        .ok_or("Admission gate missing")?
        .send(())
        .map_err(|_| "Admission gate expired".into())
}
async fn waiting(s: Signal<ShellState>) -> Result<Value, String> {
    for _ in 0..180 {
        let v = snapshot(&s.peek());
        if v["admission"]["waiting"] == true {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("Calendar admission barrier was not reached".into())
}
