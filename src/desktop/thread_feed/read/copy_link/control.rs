//! Root-lived, once-per-gesture clipboard promise driver. JSON is not policy.
use super::*;
use registry::{Outcome, Receipt, Reservation};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Config {
    pub id: u64,
    pub mount: u64,
    pub nonce: String,
    pub scope: String,
    pub host_id: String,
    pub button_id: String,
}
impl Config {
    pub(crate) fn new(id: u64, mount: u64, scope: String) -> Self {
        Self {
            id,
            mount,
            nonce: format!("thread-copy-{id}-{mount}-{scope}"),
            scope,
            host_id: format!("thread-copy-host-{id}-{mount}"),
            button_id: format!("thread-copy-button-{id}-{mount}"),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    kind: String,
    id: u64,
    mount: u64,
    nonce: String,
    seq: u64,
    outcome: Option<String>,
}
enum Bridge {
    Native(dioxus::document::Eval),
    #[cfg(test)]
    Model(test_io::ModelBridge),
}
impl Bridge {
    fn native(&self) -> bool {
        matches!(self, Self::Native(_))
    }
    async fn recv(&mut self) -> Result<Event, ()> {
        match self {
            Self::Native(eval) => eval.recv().await.map_err(|_| ()),
            #[cfg(test)]
            Self::Model(m) => serde_json::from_value(m.recv().await.ok_or(())?).map_err(|_| ()),
        }
    }
    fn send(&self, v: Value) -> Result<(), ()> {
        match self {
            Self::Native(eval) => eval.send(v).map_err(|_| ()),
            #[cfg(test)]
            Self::Model(m) => m.send(v),
        }
    }
}
fn reply(c: &Config, kind: &str, seq: u64, url: Option<&str>) -> Value {
    let mut v =
        serde_json::json!({"kind":kind,"id":c.id,"mount":c.mount,"nonce":c.nonce,"seq":seq});
    if let Some(url) = url {
        v["url"] = Value::String(url.into())
    }
    v
}
fn finish(reservation: Reservation, c: &Config, seq: u64, outcome: Outcome, native: bool) {
    reservation.finish(Receipt {
        id: c.id,
        mount: c.mount,
        seq,
        outcome,
        native,
    });
}
pub(super) fn start(state: Signal<ShellState>, id: u64, mount: u64) {
    let p = {
        let s = state.peek();
        let Some(p) = s.thread_feed_reads.copy_link.intent.as_ref() else {
            return;
        };
        if p.id != id
            || mount == 0
            || mount > 9_007_199_254_740_991
            || id > 9_007_199_254_740_991
            || p.phase.load(Ordering::Acquire) != 1
            || !p.data_current(&s)
            || p.mounted
                .compare_exchange(0, mount, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return;
        }
        p.clone()
    };
    // Allocate eval INSIDE the root task, so unmounting the button cannot
    // dispose the outstanding promise receiver before its physical outcome.
    dioxus::core::spawn_forever(async move {
        let c = Config::new(id, mount, p.action.scope.id.to_string());
        #[cfg(test)]
        let model = state.peek().thread_feed_reads.copy_link.io.clone();
        #[cfg(test)]
        if let Some(io) = model {
            let bridge = Bridge::Model(io.open(c.clone()));
            drive(state, p, c, bridge).await;
            return;
        }
        if crate::fixture::is_fixture() {
            p.fail("Clipboard access is unavailable in this fixture.");
            wake(state);
            return;
        }
        if !p.data_current(&state.peek()) {
            return;
        }
        let json = serde_json::to_string(&c).expect("owned primitive clipboard config");
        let eval = dioxus::document::eval(&format!(
            "{}\nawait runThreadCopyControl({json},globalThis,dioxus);",
            include_str!("../../../view/threads/copy_link/bridge.js")
        ));
        drive(state, p, c, Bridge::Native(eval)).await;
    });
}
async fn drive(state: Signal<ShellState>, p: Arc<Intent>, c: Config, mut bridge: Bridge) {
    let native = bridge.native();
    let mut mounted = false;
    let mut seq = 0;
    let mut awaiting = false;
    let mut reservation: Option<Reservation> = None;
    let mut warned = false;
    loop {
        let event = if !mounted {
            tokio::time::timeout(std::time::Duration::from_secs(5), bridge.recv())
                .await
                .unwrap_or(Err(()))
        } else if reservation.is_some() && !warned {
            match tokio::time::timeout(std::time::Duration::from_secs(30), bridge.recv()).await {
                Ok(e) => e,
                Err(_) => {
                    reservation.as_ref().unwrap().pending_unknown(Receipt {
                        id: c.id,
                        mount: c.mount,
                        seq,
                        outcome: Outcome::Unknown,
                        native,
                    });
                    warned = true;
                    p.notice("The clipboard update is still unconfirmed. Waiting for its original result.");
                    wake(state);
                    continue;
                }
            }
        } else {
            bridge.recv().await
        };
        let Ok(e) = event else {
            if let Some(r) = reservation.take() {
                finish(r, &c, seq, Outcome::Unknown, native)
            }
            p.fail(if awaiting {
                "The clipboard update is unconfirmed. No link will be copied again automatically."
            } else {
                "The clipboard control could not be verified. Prepare the link again."
            });
            break;
        };
        if e.id != c.id || e.mount != c.mount || e.nonce != c.nonce {
            if let Some(r) = reservation.take() {
                finish(r, &c, seq, Outcome::Unknown, native)
            }
            p.fail("The clipboard response could not be verified.");
            break;
        }
        match e.kind.as_str() {
            "mounted" if !mounted && e.seq == 0 && e.outcome.is_none() => {
                mounted = true;
                if p.data_current(&state.peek()) && p.mounted.load(Ordering::Acquire) == c.mount {
                    p.native_ready.store(true, Ordering::Release);
                    p.dirty.store(true, Ordering::Release);
                    wake(state);
                } else {
                    p.fail("This message changed. Prepare its link again.");
                    wake(state);
                }
            }
            "unsupported" if !mounted && e.seq == 0 && e.outcome.is_none() => {
                p.fail("Clipboard access is unavailable in this window.");
                break;
            }
            "retired" if !awaiting && e.seq == seq && e.outcome.is_none() => {
                p.closed.store(true, Ordering::Release);
                p.native_ready.store(false, Ordering::Release);
                break;
            }
            "pressed" if mounted && !awaiting && e.outcome.is_none() && e.seq == seq + 1 => {
                seq = e.seq;
                awaiting = true;
                warned = false;
                let grant = {
                    let s = state.peek();
                    if p.mounted.load(Ordering::Acquire) != c.mount
                        || p.phase.load(Ordering::Acquire) != 1
                        || !p.data_current(&s)
                    {
                        None
                    } else if let Some(r) = p.ledger.claim() {
                        let used = p.watch.with_current(|| {
                            p.phase
                                .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire)
                                .is_ok()
                        }) == Some(true);
                        if used {
                            let url = p
                                .ready
                                .lock()
                                .ok()
                                .and_then(|r| r.as_ref().map(|r| r.link.url().to_owned()));
                            if let Some(url) = url {
                                Some((r, url))
                            } else {
                                finish(r, &c, seq, Outcome::NotRequested, native);
                                None
                            }
                        } else {
                            finish(r, &c, seq, Outcome::NotRequested, native);
                            None
                        }
                    } else {
                        None
                    }
                };
                if let Some((r, url)) = grant {
                    reservation = Some(r);
                    // Ingress may retire this watch independently of the
                    // serial UI. Seal the final URL publication under its
                    // lease too, without awaiting or reentering Shell state.
                    let current = p.data_current(&state.peek());
                    let published = current
                        .then(|| {
                            p.watch
                                .with_current(|| bridge.send(reply(&c, "permit", seq, Some(&url))))
                        })
                        .flatten();
                    if published == Some(Ok(())) {
                        p.notice("Copying message link…");
                        wake(state);
                    } else if published.is_none() {
                        finish(
                            reservation.take().unwrap(),
                            &c,
                            seq,
                            Outcome::NotRequested,
                            native,
                        );
                        p.fail("This message changed. Prepare its link again.");
                        if bridge.send(reply(&c, "deny", seq, None)).is_err() {
                            break;
                        }
                    } else {
                        finish(
                            reservation.take().unwrap(),
                            &c,
                            seq,
                            Outcome::Unknown,
                            native,
                        );
                        p.fail("The clipboard update is unconfirmed. No link will be copied again automatically.");
                        break;
                    }
                } else if bridge.send(reply(&c, "deny", seq, None)).is_err() {
                    p.fail("The clipboard control is unavailable.");
                    break;
                }
            }
            "settled" if mounted && awaiting && e.seq == seq => {
                let outcome = match e.outcome.as_deref() {
                    Some("fulfilled") => Outcome::Fulfilled,
                    Some("rejected") => Outcome::Rejected,
                    Some("not_requested") => Outcome::NotRequested,
                    _ => Outcome::Unknown,
                };
                if let Some(r) = reservation.take() {
                    finish(r, &c, seq, outcome, native)
                } else if outcome != Outcome::NotRequested {
                    p.fail("The clipboard response could not be verified.");
                    break;
                }
                awaiting = false;
                if outcome == Outcome::Unknown {
                    p.fail("The clipboard update is unconfirmed. No link will be copied again automatically.");
                } else if p.data_current(&state.peek()) {
                    p.phase.store(1, Ordering::Release);
                    p.notice(match outcome {
                        Outcome::Fulfilled => "Message link copied.",
                        Outcome::Rejected => {
                            "The clipboard could not be updated. Choose Copy link to try again."
                        }
                        _ => "The link was not copied. Choose Copy link for a new attempt.",
                    });
                } else {
                    p.closed.store(true, Ordering::Release);
                    p.native_ready.store(false, Ordering::Release);
                }
                // Record/release precedes ACK. This message acknowledges delivery
                // only; it is never a new copying permit or automatic retry.
                if bridge.send(reply(&c, "ack", seq, None)).is_err() {
                    // Keep the already recorded physical outcome. Retire the
                    // control so its disabled keyed node can be prepared anew.
                    p.fail(
                        "The clipboard control closed after its result. Prepare the link again.",
                    );
                    break;
                }
                wake(state);
                if outcome == Outcome::Unknown {
                    break;
                }
            }
            _ => {
                if let Some(r) = reservation.take() {
                    finish(r, &c, seq, Outcome::Unknown, native)
                }
                p.fail("The clipboard response could not be verified.");
                break;
            }
        }
    }
    p.native_ready.store(false, Ordering::Release);
    p.dirty.store(true, Ordering::Release);
    wake(state);
}
