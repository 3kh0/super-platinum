//! Same files.info transport, with only three source-proved error producers.
use super::*;
use crate::slack::api::custom::texty::file_title_source::FileReferenceResponse;

impl Transport {
    pub async fn execute_file_reference_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<FileReferenceResponse, Error> {
        if !allowed(&req) {
            return Err(Error::Api("invalid_file_info_request".into()));
        }
        let RequestBody::Form(fields) = &req.body else {
            unreachable!()
        };
        let id = fields
            .iter()
            .find(|(key, _)| key == "file")
            .expect("validated file")
            .1
            .clone();
        self.file_reference_admitted(&req, &id, &current).await
    }

    async fn file_reference_admitted(
        &self,
        req: &PreparedRequest,
        id: &str,
        current: &impl Fn() -> bool,
    ) -> Result<FileReferenceResponse, Error> {
        let mut attempt = 0;
        let mut last_failure = None;
        loop {
            if !current() {
                return Err(retired(last_failure));
            }
            let link = self.await_link().await;
            if !current() {
                return Err(retired(last_failure));
            }
            link?;
            let result = if req.retry_safe() {
                tokio::select! {
                    biased;
                    result = self.file_reference_once(req, id) => result,
                    () = self.link_dropped() => Err(Error::Offline("link dropped mid-call".into())),
                }
            } else {
                self.file_reference_once(req, id).await
            };
            // A response, including a status producer, never outlives its owner.
            if !current() {
                return Err(retired(None));
            }
            match result {
                Ok(value) => return Ok(value),
                Err(error) if req.retry_safe() && retryable_error(&error) && attempt < 2 => {
                    attempt += 1;
                    last_failure = Some(error.clone());
                    tokio::time::sleep(retry_delay(&error, attempt)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn file_reference_once(
        &self,
        req: &PreparedRequest,
        id: &str,
    ) -> Result<FileReferenceResponse, Error> {
        let mut bytes = None;
        let result = self
            .execute_once_using_observed_decode_mode(
                req,
                false,
                self.http()?,
                false,
                None,
                None,
                Some(&mut bytes),
            )
            .await;
        match &result {
            Ok(_) => {}
            Err(Error::Api(code))
                if matches!(
                    code.as_str(),
                    "not_visible" | "template_not_visible" | "file_deleted"
                ) => {}
            Err(_) => return result.map(|_| unreachable!()),
        }
        let bytes =
            bytes.ok_or_else(|| Error::Transport("missing original file response bytes".into()))?;
        let response = FileReferenceResponse::decode(id, bytes.as_bytes())?;
        if let Ok(raw) = result {
            if response.raw() != &raw {
                return Err(Error::Transport(
                    "file response observation mismatch".into(),
                ));
            }
        }
        Ok(response)
    }
}
fn retired(previous: Option<Error>) -> Error {
    previous.unwrap_or_else(|| Error::Offline("File info owner retired".into()))
}

#[cfg(test)]
mod tests;
