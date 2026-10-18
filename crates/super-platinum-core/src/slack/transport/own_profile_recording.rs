//! Private pronunciation upload. Each stage executes once and never redirects.
use super::*;
use crate::slack::api::recording::{self, UploadTicket};

impl Transport {
    pub async fn execute_name_recording(&self, request: PreparedRequest) -> Result<Value, Error> {
        if !allowed(&request) {
            return Err(Error::Api("invalid_name_recording_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Recording operation was not sent while offline".into(),
            ));
        }
        let result = self
            .execute_once_using(&request, true, self.single_attempt_client()?)
            .await
            .map_err(|e| match e {
                Error::Offline(_) => {
                    Error::Transport("Recording response lost after dispatch".into())
                }
                e => e,
            })?;
        recording::success(&result)?;
        Ok(result)
    }
    pub async fn upload_name_recording(
        &self,
        ticket: &UploadTicket,
        bytes: Vec<u8>,
        mime: &str,
    ) -> Result<(), Error> {
        if bytes.is_empty()
            || bytes.len() > recording::MAX_BYTES
            || !matches!(mime, "audio/mp4" | "audio/webm;codecs=opus" | "audio/webm")
        {
            return Err(Error::Api("invalid_name_recording_audio".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Recording bytes were not sent while offline".into(),
            ));
        }
        // The capability URL is validated by the private ticket constructor.
        // Cookies and authorization headers must never accompany this POST.
        self.http()?;
        let boundary = format!("super-platinum-recording-{}", uuid::Uuid::new_v4().simple());
        let mut body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: {mime}\r\n\r\n",recording::FILENAME).into_bytes();
        body.extend(bytes);
        body.extend(format!("\r\n--{boundary}--\r\n").into_bytes());
        let response = self
            .single_attempt_client()?
            .post(ticket.url())
            .timeout(API_TIMEOUT)
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body)
            .send()
            .await
            .map_err(|_| Error::Transport("Recording upload response was lost".into()))?;
        let retry_after_secs = retry_after_secs(response.headers());
        if response.status().as_u16() == 429 {
            return Err(Error::RateLimited { retry_after_secs });
        }
        if response.status().as_u16() != 200 {
            return Err(Error::HttpStatus {
                status: response.status().as_u16(),
                retry_after_secs,
            });
        }
        Ok(())
    }
}
fn allowed(request: &PreparedRequest) -> bool {
    let Ok(url) = url::Url::parse(&request.url) else {
        return false;
    };
    let origin = url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.host_str().is_some_and(|h| h.ends_with(".slack.com"));
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || request.method != "POST"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    let RequestBody::Form(fields) = &request.body else {
        return false;
    };
    let get = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let keys: &[&str] = match url.path() {
        "/api/files.getUploadURL" => &["token", "filename", "length", "subtype", "_x_reason"],
        "/api/files.completeUpload" => &["token", "files", "_x_reason"],
        _ => return false,
    };
    if fields.len() != keys.len()
        || keys
            .iter()
            .any(|key| fields.iter().filter(|(k, _)| k == key).count() != 1)
        || get("_x_reason") != Some("upload-queue")
    {
        return false;
    }
    if url.path().ends_with("getUploadURL") {
        get("filename") == Some(recording::FILENAME)
            && get("subtype") == Some(recording::SUBTYPE)
            && get("length")
                .and_then(|v| v.parse::<usize>().ok())
                .is_some_and(|n| n > 0 && n <= recording::MAX_BYTES)
    } else {
        get("files")
            .and_then(|v| serde_json::from_str::<Value>(v).ok())
            .is_some_and(|v| {
                v.as_array().is_some_and(|a| {
                    a.len() == 1
                        && a[0].as_object().is_some_and(|o| o.len() == 3)
                        && a[0]["title"] == recording::FILENAME
                        && a[0]["subtype"] == recording::SUBTYPE
                        && a[0]["id"].as_str().is_some_and(recording::valid_file)
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workspace() -> crate::config::WorkspaceSession {
        crate::config::WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        }
    }
    #[test]
    fn recording_transport_only_accepts_closed_private_upload_forms() {
        let c = crate::slack::SlackClient::default();
        let w = workspace();
        let ticket=UploadTicket::decode(&serde_json::json!({"ok":true,"file":"F00000001","upload_url":"https://files.slack.com/upload/synthetic"})).unwrap();
        let requests = [
            recording::ticket_request(&c, &w, 120).unwrap(),
            recording::complete_request(&c, &w, &ticket),
        ];
        for request in requests {
            assert!(allowed(&request));
            let mut bad = request.clone();
            let RequestBody::Form(fields) = &mut bad.body else {
                panic!("form")
            };
            fields.push(("channel".into(), "C00000001".into()));
            assert!(!allowed(&bad));
            let mut bad = request.clone();
            bad.url = "https://evil.test/api/files.completeUpload".into();
            assert!(!allowed(&bad));
            let mut bad = request.clone();
            bad.method = "GET".into();
            assert!(!allowed(&bad));
            let mut bad = request;
            let RequestBody::Form(fields) = &mut bad.body else {
                panic!("form")
            };
            fields.push(("token".into(), "another".into()));
            assert!(!allowed(&bad));
        }
    }
}
