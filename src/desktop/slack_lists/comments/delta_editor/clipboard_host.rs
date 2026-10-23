//! Finite host dispatch for bytes already supplied by a native paste event.
//! No system clipboard API, network factory or missing-to-false policy lives
//! here. The caller owns source acquisition and the original native capture.
use super::{EmojiAppearance, clipboard_protocol::Request};
use serde_json::Value;
use std::{future::Future, rc::Rc, time::Duration};
use super_platinum_core::slack::Error;

const DEADLINE: Duration = Duration::from_secs(4);

fn unavailable() -> Error {
    Error::Api("list_comment_delta_clipboard_unavailable".into())
}

pub(crate) struct Observations {
    validity: Vec<(String, bool)>,
    emoji: Vec<EmojiAppearance>,
    current: Rc<dyn Fn() -> bool>,
}
impl Observations {
    /// The callback must retain actual source witnesses, not recapture successor
    /// data. A successfully acquired absence may be false; unknown must fail.
    pub(crate) fn new(
        validity: Vec<(String, bool)>,
        emoji: Vec<EmojiAppearance>,
        current: Rc<dyn Fn() -> bool>,
    ) -> Self {
        Self {
            validity,
            emoji,
            current,
        }
    }
}

pub(crate) struct Prepared {
    request: Request,
    observations: Observations,
}
impl Prepared {
    /// Immediately before sending on the retained Eval, recheck both original
    /// native ownership and all independently acquired semantic witnesses.
    pub(crate) fn receipt(&self, current: impl Fn() -> bool) -> Result<Value, Error> {
        if !current() || !(self.observations.current)() {
            return Err(unavailable());
        }
        self.request
            .receipt(
                self.observations.validity.clone(),
                self.observations.emoji.clone(),
            )
            .ok_or_else(unavailable)
    }
}

/// Run in a separate cancellable task: the original capture/ACK loop must keep
/// processing input, retirement and final custody while acquisition is pending.
/// Parent drops this future on the exact clipboard-cancel or mount retirement.
pub(crate) async fn dispatch(
    request: Request,
    current: impl Fn() -> bool,
    acquire: impl Future<Output = Result<Observations, Error>>,
) -> Result<Prepared, Error> {
    dispatch_until(request, current, acquire, DEADLINE).await
}

async fn dispatch_until(
    request: Request,
    current: impl Fn() -> bool,
    acquire: impl Future<Output = Result<Observations, Error>>,
    deadline: Duration,
) -> Result<Prepared, Error> {
    if !current() {
        return Err(unavailable());
    }
    let observations = tokio::time::timeout(deadline, acquire)
        .await
        .map_err(|_| unavailable())??;
    let prepared = Prepared {
        request,
        observations,
    };
    // Validate ordered completeness before returning; retain witnesses for the
    // later send boundary rather than turning this into an unguarded Value.
    prepared.receipt(&current)?;
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;

    fn request() -> Request {
        Request::decode(
            json!({"kind":"clipboard-request","scope":"original","mount":"mount",
            "generation":3,"sequence":8,"clipboard":{
            "request":"ed0f63eb-63d5-4b3c-92c6-8985471fcae8","scope":"original","generation":"3",
            "revision":4,"selectionRevision":2,"selection":{"index":1,"length":0},
            "ops":[{"insert":"@Ada","attributes":{"slackmention":{"id":"U1","label":"@Ada"}}}],
            "mentionIds":["U1"],"emojiNames":[]}}),
        )
        .unwrap()
    }
    fn facts(current: Rc<dyn Fn() -> bool>) -> Observations {
        Observations::new(vec![("U1".into(), true)], vec![], current)
    }
    #[tokio::test]
    async fn stale_owner_never_polls_acquisition() {
        let polled = Cell::new(false);
        assert!(
            dispatch(request(), || false, async {
                polled.set(true);
                Ok(facts(Rc::new(|| true)))
            })
            .await
            .is_err()
        );
        assert!(!polled.get());
    }
    #[tokio::test]
    async fn prepared_receipt_rechecks_retained_facts_and_native_owner() {
        let valid = Rc::new(Cell::new(true));
        let observed = valid.clone();
        let prepared = dispatch(request(), || true, async {
            Ok(facts(Rc::new(move || observed.get())))
        })
        .await
        .unwrap();
        assert_eq!(
            prepared.receipt(|| true).unwrap()["receipt"]["validity"],
            json!([["U1", true]])
        );
        assert!(prepared.receipt(|| false).is_err());
        valid.set(false);
        assert!(prepared.receipt(|| true).is_err());
    }
    #[tokio::test]
    async fn unavailable_rows_are_not_negative_and_deadline_cancels_future() {
        let missing = Observations::new(vec![], vec![], Rc::new(|| true));
        assert!(
            dispatch(request(), || true, async { Ok(missing) })
                .await
                .is_err()
        );
        let dropped = Rc::new(Cell::new(false));
        struct DropFlag(Rc<Cell<bool>>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }
        let guard = DropFlag(dropped.clone());
        let acquire = async move {
            let _guard = guard;
            std::future::pending::<Result<Observations, Error>>().await
        };
        assert!(
            dispatch_until(request(), || true, acquire, Duration::from_millis(1))
                .await
                .is_err()
        );
        assert!(dropped.get());
    }
}
