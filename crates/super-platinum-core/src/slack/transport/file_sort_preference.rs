//! Closed text multipart mutation. Qualified native policy: one bounded attempt,
//! no redirects, queue, persistence, replay or automatic retry. A lost reply is
//! unconfirmed; it must never be reported as definitely not saved.
use super::*;
use crate::slack::api::file_sort_preference::{self, Request};

#[derive(Debug)]
pub enum FileSortPreferenceFailure {
    NotRequested(Error),
    Unconfirmed(Error),
}
impl FileSortPreferenceFailure {
    pub fn error(&self) -> &Error {
        match self {
            Self::NotRequested(e) | Self::Unconfirmed(e) => e,
        }
    }
    pub fn into_error(self) -> Error {
        match self {
            Self::NotRequested(e) | Self::Unconfirmed(e) => e,
        }
    }
}
impl Transport {
    pub async fn execute_file_sort_preference_current(
        &self,
        request: &Request,
        mut current: impl FnMut() -> bool,
        mut observe: impl FnMut(&Result<Value, Error>),
    ) -> Result<Value, FileSortPreferenceFailure> {
        use FileSortPreferenceFailure::{NotRequested, Unconfirmed};
        if !allowed(request) || !current() {
            return Err(NotRequested(unavailable()));
        }
        if self.health() == Health::Offline {
            return Err(NotRequested(Error::Offline(
                "File sort preference was not sent while offline".into(),
            )));
        }
        let RequestBody::Form(fields) = &request.inner.body else {
            return Err(NotRequested(unavailable()));
        };
        let form = fields
            .iter()
            .fold(wreq::multipart::Form::new(), |form, (k, v)| {
                form.text(k.clone(), v.clone())
            });
        let http = self.single_attempt_client().map_err(NotRequested)?;
        let mut builder = http.post(&request.inner.url).timeout(API_TIMEOUT);
        for (key, value) in &request.inner.headers {
            if key.eq_ignore_ascii_case("content-type") {
                return Err(NotRequested(unavailable()));
            }
            builder = builder.header(key.as_str(), value.as_str());
        }
        let wire = builder
            .header("Cookie", self.cookie())
            .multipart(form)
            .build()
            .map_err(|_| NotRequested(unavailable()))?;
        if self.health() == Health::Offline {
            return Err(NotRequested(Error::Offline(
                "File sort preference was not sent while offline".into(),
            )));
        }
        if !current() {
            return Err(NotRequested(unavailable()));
        }
        let result = match tokio::time::timeout(API_TIMEOUT, async {
            let response = http.execute(wire).await.map_err(|_| {
                self.set_health(Health::Offline);
                Error::Transport("File sort preference reply was lost after dispatch".into())
            })?;
            self.set_health(Health::Online);
            let status = response.status().as_u16();
            let retry_after_secs = retry_after_secs(response.headers());
            if status == 429 {
                return Err(Error::RateLimited { retry_after_secs });
            }
            if !(200..300).contains(&status) || retry_after_secs.is_some() {
                return Err(Error::HttpStatus {
                    status,
                    retry_after_secs,
                });
            }
            let bytes = read_media_body(response, 1024 * 1024)
                .await
                .map_err(|_| Error::Transport("File sort preference body unavailable".into()))?;
            let value = crate::slash_commands::codec::decode(&bytes, 1024 * 1024)
                .map_err(|_| Error::Transport("File sort preference JSON unavailable".into()))?;
            file_sort_preference::validate_response(&value)?;
            Ok(value)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(Error::Transport(
                "File sort preference reply timed out after dispatch".into(),
            )),
        };
        // Observe an actual attempted rate outcome even if the UI/source owner
        // retired. Return receipt evidence; only the host may publish, after
        // checking its exact intent/producer. No second dispatch is possible.
        observe(&result);
        result.map_err(Unconfirmed)
    }
}
fn unavailable() -> Error {
    Error::Api("file_sort_preference_not_admitted".into())
}
fn allowed(request: &Request) -> bool {
    let Ok(url) = url::Url::parse(&request.inner.url) else {
        return false;
    };
    let secure = url.scheme() == "https"
        && request.destination.ends_with(".slack.com")
        && url.port().is_none_or(|p| p == 443);
    #[cfg(test)]
    let secure = secure || (url.scheme() == "http" && request.destination == "127.0.0.1");
    if !secure
        || url.host_str() != Some(request.destination.as_str())
        || url.path() != "/api/users.prefs.set"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || request.inner.method != "POST"
    {
        return false;
    }
    let RequestBody::Form(fields) = &request.inner.body else {
        return false;
    };
    fields.len() == 4
        && fields
            .iter()
            .zip(["token", "name", "value", "_x_reason"])
            .all(|((k, _), expected)| k == expected)
        && !fields[0].1.is_empty()
        && fields[0].1.len() <= 16 * 1024
        && fields[1].1 == "search_file_sort"
        && fields[2].1 == request.sort.raw()
        && matches!(
            fields[2].1.as_str(),
            "score" | "timestamp_asc" | "created" | "name" | "name_desc"
        )
        && fields[3].1 == "prefs-api/setUserPrefByApi"
}

#[cfg(test)]
mod tests;
