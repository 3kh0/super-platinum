//! Dispatch admission and account-owned observation are distinct from retirement.
use super::*;

impl Transport {
    /// None means dispatch was refused after link readiness; no attempt occurred.
    /// The observer sees completed attempts before the original retirement fence.
    pub async fn execute_admitted(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
        mut admit: impl FnMut() -> bool,
        mut observe: impl FnMut(&Result<Value, Error>),
    ) -> Result<Option<Value>, Error> {
        let mut attempt = 0;
        loop {
            if !current() {
                return Err(Error::Api("request_superseded".into()));
            }
            let link = self.await_link().await;
            if !current() {
                return Err(Error::Api("request_superseded".into()));
            }
            link?;
            if !admit() {
                return Ok(None);
            }
            // A call already on the wire when the link goes has nothing to wait
            // for but its own timeout. Cutting it loose the moment the shell
            // says the network is gone is what keeps a pane from sitting on
            // "Loading…" for half a minute; the retry below re-enters the hold
            // and goes out again once the link is confirmed back. Only for
            // calls that are safe to repeat — a send that may have landed must
            // not be fired twice.
            let attempted = if req.retry_safe() {
                tokio::select! {
                    biased;
                    result = self.execute_once(&req) => result,
                    () = self.link_dropped() => {
                        Err(Error::Offline("link dropped mid-call".into()))
                    }
                }
            } else {
                self.execute_once(&req).await
            };
            observe(&attempted);
            if !current() {
                return Err(Error::Api("request_superseded".into()));
            }
            match attempted {
                Ok(value) => return Ok(Some(value)),
                Err(e) if req.retry_safe() && retryable_error(&e) && attempt < 2 => {
                    attempt += 1;
                    tokio::time::sleep(retry_delay(&e, attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests;
