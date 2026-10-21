//! A single original metadata read. The consumer owns Retry-After and explicit
//! retry, so generic retry backoff must not hide, shorten or replay a 429.
use super::*;

impl Transport {
    pub async fn execute_file_metadata_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<Value, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_file_metadata_request".into()));
        }
        self.file_metadata_admitted(&req, current).await
    }

    async fn file_metadata_admitted(
        &self,
        req: &PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<Value, Error> {
        let retired = || Error::Offline("File metadata owner retired".into());
        if !current() {
            return Err(retired());
        }
        let work = async {
            self.await_link().await?;
            if !current() {
                return Err(retired());
            }
            let mut original_bytes = None;
            // Requesting the observed bytes selects strict UTF8 decoding; the
            // display subset needs the raw envelope, not ordered channel maps.
            tokio::select! {
                biased;
                value = self.execute_once_using_observed_decode_mode(
                    req, false, self.http()?, true, None, None, Some(&mut original_bytes)
                ) => value,
                () = self.link_dropped() => Err(Error::Offline("link dropped mid-call".into())),
            }
        };
        let result = tokio::select! {
            biased;
            _ = async {
                loop {
                    if !current() { break; }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            } => Err(retired()),
            result = work => result,
        };
        if !current() {
            return Err(retired());
        }
        result
    }
}

fn allowed(req: &PreparedRequest) -> bool {
    let RequestBody::Form(fields) = &req.body else {
        return false;
    };
    let cold = fields.len() == 7
        && fields
            .iter()
            .filter(|(k, v)| k == "skip_shares" && v == "true")
            .count()
            == 1
        && fields
            .iter()
            .filter(|(k, v)| k == "_x_reason" && v == "files-store-unknown-fetch")
            .count()
            == 1;
    let known = fields.len() == 6
        && !fields.iter().any(|(k, _)| k == "skip_shares")
        && fields
            .iter()
            .filter(|(k, v)| k == "_x_reason" && v == "files-store.ConditionalFetchManager.fetch")
            .count()
            == 1;
    if !(cold || known)
        || !fields.iter().any(|(k, id)| {
            k == "file"
                && (9..=15).contains(&id.len())
                && id.starts_with('F')
                && id[1..]
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        })
    {
        return false;
    }
    // Reuse the established exact method, endpoint, token, page and truncation
    // admission after removing only the proven manager metadata arguments.
    let mut base = req.clone();
    if let RequestBody::Form(fields) = &mut base.body {
        fields.retain(|(key, _)| key != "skip_shares" && key != "_x_reason");
    }
    super::allowed(&base)
}

#[cfg(test)]
mod tests;
