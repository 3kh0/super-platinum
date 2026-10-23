//! Closed owner-bound channel metadata read after a restricted Share failure.
use super::*;
use crate::workflow_share::posting_refresh::REASON;

impl Transport {
    pub async fn execute_workflow_posting_refresh_current(
        &self,
        request: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<Value, Error> {
        if !allowed(&request) {
            return Err(Error::Api(
                "invalid_workflow_posting_refresh_request".into(),
            ));
        }
        let retired = || Error::Offline("Workflow posting refresh owner retired".into());
        let work = async {
            let mut attempt = 0;
            loop {
                if !current() {
                    return Err(retired());
                }
                self.await_link().await?;
                if !current() {
                    return Err(retired());
                }
                // Preserve the existing read decoder/retry classification but
                // never follow a redirect with the original credential body.
                let http = self.single_attempt_client()?;
                let result = tokio::select! {
                    biased;
                    result=self.execute_once_using(&request,false,http)=>result,
                    ()=self.link_dropped()=>Err(Error::Offline("link dropped mid-call".into())),
                };
                match result {
                    Ok(raw) => return Ok(raw),
                    Err(e) if retryable_error(&e) && attempt < 2 => {
                        attempt += 1;
                        tokio::time::sleep(retry_delay(&e, attempt)).await;
                    }
                    Err(e) => return Err(e),
                }
            }
        };
        tokio::select! {
            biased;
            _=async { loop { if !current() {break;} tokio::time::sleep(Duration::from_millis(100)).await; } }=>Err(retired()),
            result=work=>result,
        }
    }
}
fn allowed(r: &PreparedRequest) -> bool {
    if r.method != "POST" || !r.retry_safe() {
        return false;
    }
    let RequestBody::Form(fields) = &r.body else {
        return false;
    };
    if fields.len() != 3
        || fields[0].0 != "token"
        || fields[0].1.is_empty()
        || fields[1].0 != "channel"
        || !crate::workflow_share::posting_refresh::valid_channel(&fields[1].1)
        || fields[2] != ("_x_reason".into(), REASON.into())
    {
        return false;
    }
    url::Url::parse(&r.url).is_ok_and(|u| {
        let origin = u.scheme() == "https"
            && u.host_str().is_some_and(|h| h.ends_with(".slack.com"))
            && u.port_or_known_default() == Some(443);
        #[cfg(test)]
        let origin = origin || (u.scheme() == "http" && u.host_str() == Some("127.0.0.1"));
        origin
            && u.username().is_empty()
            && u.password().is_none()
            && u.fragment().is_none()
            && u.path() == "/api/conversations.info"
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> PreparedRequest {
        let s = crate::config::WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        };
        crate::workflow_share::posting_refresh::request(
            &crate::slack::SlackClient::default(),
            &s,
            "CDEST0001",
        )
        .unwrap()
    }
    #[test]
    fn exact_request_does_not_admit_generic_flags_or_other_methods() {
        let good = request();
        assert!(allowed(&good));
        let RequestBody::Form(f) = &good.body else {
            panic!()
        };
        assert_eq!(
            &f[1..],
            &[
                ("channel".into(), "CDEST0001".into()),
                ("_x_reason".into(), REASON.into())
            ]
        );
        let mut bad = request();
        if let RequestBody::Form(f) = &mut bad.body {
            f.push(("include_num_members".into(), "false".into()));
        }
        assert!(!allowed(&bad));
        let mut bad = request();
        bad.url = bad.url.replace("conversations.info", "chat.postMessage");
        assert!(!allowed(&bad));
        let mut bad = request();
        if let RequestBody::Form(f) = &mut bad.body {
            f[2].1 = "other".into();
        }
        assert!(!allowed(&bad));
    }
    #[tokio::test]
    async fn retired_original_read_never_waits_for_link_or_dispatches() {
        let t = Transport::disconnected_fixture("synthetic");
        assert!(matches!(
            t.execute_workflow_posting_refresh_current(request(), || false)
                .await,
            Err(Error::Offline(_))
        ));
    }
}
