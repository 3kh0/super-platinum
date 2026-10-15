//! Candidate child of slack::transport. Requires sealed one-use Permit.
//! No module declaration or runtime call site is supplied by this prep.
use super::*;
use crate::slash_commands::controller::{Delivery, Permit, Report};

impl Transport {
    pub async fn execute_slash_command(&self, permit: Permit) -> Report {
        if !permit.current(self)
            || self.health() == Health::Offline
            || !slash_request(permit.request())
        {
            return permit.report(Delivery::NotSent, None);
        }
        let Ok(http) = self.single_attempt_client() else {
            return permit.report(Delivery::NotSent, None);
        };
        let RequestBody::Form(fields) = &permit.request().body else {
            return permit.report(Delivery::NotSent, None);
        };
        let mut builder = http
            .post(&permit.request().url)
            .timeout(API_TIMEOUT)
            .redirect(wreq::redirect::Policy::none());
        for (key, value) in &permit.request().headers {
            if !key.eq_ignore_ascii_case("content-type") {
                builder = builder.header(key.as_str(), value.as_str());
            }
        }
        let Ok(request) = builder.header("Cookie", self.cookie()).form(fields).build() else {
            return permit.report(Delivery::NotSent, None);
        };
        // Final no-await fence. The trusted admission host must revoke promptly.
        if !permit.current(self) || self.health() == Health::Offline {
            return permit.report(Delivery::NotSent, None);
        }
        let response = match http.execute(request).await {
            Ok(v) => v,
            Err(error) => {
                if send_error(error).is_offline() {
                    self.set_health(Health::Offline);
                }
                return permit.report(Delivery::Unknown, None);
            }
        };
        self.set_health(Health::Online);
        let retry = slash_retry_after(response.headers());
        if !response.status().is_success() {
            return permit.report(Delivery::HttpFailure(response.status().as_u16()), retry);
        }
        const MAX: usize = 256 * 1024;
        if response.content_length().is_some_and(|v| v > MAX as u64) {
            return permit.report(Delivery::Unknown, retry);
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else {
                return permit.report(Delivery::Unknown, retry);
            };
            if chunk.len() > MAX - bytes.len() {
                return permit.report(Delivery::Unknown, retry);
            }
            bytes.extend_from_slice(&chunk);
        }
        let delivery = crate::slash_commands::codec::decode(&bytes, MAX)
            .map(Delivery::Reply)
            .unwrap_or(Delivery::Unknown);
        permit.report(delivery, retry)
    }
}
fn slash_request(request: &PreparedRequest) -> bool {
    if request.method != "POST" {
        return false;
    }
    let RequestBody::Form(fields) = &request.body else {
        return false;
    };
    if fields
        .iter()
        .try_fold(0usize, |n, (k, v)| {
            n.checked_add(k.len())?.checked_add(v.len())
        })
        .is_none_or(|n| n > 256 * 1024)
    {
        return false;
    }
    url::Url::parse(&request.url).is_ok_and(|url| {
        url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|host| host.ends_with(".slack.com"))
            && url.port_or_known_default() == Some(443)
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
            && url.path() == "/api/chat.command"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_guard_keeps_client_xparams_but_rejects_other_routes() {
        let request = |url: &str| PreparedRequest {
            method: "POST",
            url: url.into(),
            headers: vec![],
            body: RequestBody::Form(vec![]),
        };
        assert!(slash_request(&request(
            "https://synthetic.slack.com/api/chat.command?_x_app_name=client"
        )));
        for url in [
            "https://synthetic.slack.com/api/chat.postMessage",
            "https://synthetic.slack.com.evil.invalid/api/chat.command",
            "http://synthetic.slack.com/api/chat.command",
            "https://u:p@synthetic.slack.com/api/chat.command",
            "https://synthetic.slack.com:444/api/chat.command",
            "https://synthetic.slack.com/api/chat.command#x",
        ] {
            assert!(!slash_request(&request(url)));
        }
    }
    #[test]
    fn request_size_and_post_form_are_required() {
        let mut req = PreparedRequest {
            method: "GET",
            url: "https://synthetic.slack.com/api/chat.command".into(),
            headers: vec![],
            body: RequestBody::Form(vec![]),
        };
        assert!(!slash_request(&req));
        req.method = "POST";
        req.body = RequestBody::Form(vec![("blocks".into(), "x".repeat(256 * 1024))]);
        assert!(!slash_request(&req));
    }
}

// This mutation accepts one unambiguous raw ASCII decimal header only. Invalid
// or repeated fields become None; a 429 then creates a permanent journal hold.
fn slash_retry_after(headers: &wreq::header::HeaderMap) -> Option<u64> {
    let mut values = headers.get_all(wreq::header::RETRY_AFTER).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    let text = value.to_str().ok()?;
    if text.is_empty() || !text.bytes().all(|v| v.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}
#[cfg(test)]
mod strict_retry_tests {
    use super::*;
    #[test]
    fn retry_after_requires_raw_unsigned_decimal_without_coercion() {
        for (text, expected) in [
            ("0", Some(0)),
            ("37", Some(37)),
            ("", None),
            ("+0", None),
            ("-1", None),
            ("1.0", None),
            (" 0", None),
            ("0 ", None),
            ("18446744073709551616", None),
        ] {
            let mut headers = wreq::header::HeaderMap::new();
            headers.insert(
                wreq::header::RETRY_AFTER,
                wreq::header::HeaderValue::from_str(text).unwrap(),
            );
            assert_eq!(slash_retry_after(&headers), expected);
        }
    }
    #[test]
    fn missing_or_duplicate_retry_after_is_unknown_even_when_equal() {
        let mut headers = wreq::header::HeaderMap::new();
        assert_eq!(slash_retry_after(&headers), None);
        headers.append(
            wreq::header::RETRY_AFTER,
            wreq::header::HeaderValue::from_static("0"),
        );
        headers.append(
            wreq::header::RETRY_AFTER,
            wreq::header::HeaderValue::from_static("0"),
        );
        assert_eq!(slash_retry_after(&headers), None);
    }
}
