//! Closed source/read/write oracle owned by one fresh temporary queue.
//! No account/path/response setter or live transport fallback is accepted.
use super::*;
use crate::drafts::DraftKey;
use crate::slack::{PreparedRequest, client::RequestBody};
use serde_json::json;
use std::sync::Mutex;
pub const HOSTED_FIXTURE_CAPTION: &str = "Hosted files 日本語 e\u{301}";
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HostedFixtureCase {
    Caption,
    FileOnly,
    Unknown,
    Storage,
}
pub struct HostedFixture {
    queue: Arc<Queue>,
    scope: Scope,
    case: HostedFixtureCase,
    reads: Mutex<Vec<Value>>,
    request: Mutex<Option<Value>>,
}
impl HostedFixture {
    pub(in crate::drafts::store) fn create(
        queue: Arc<Queue>,
        case: HostedFixtureCase,
    ) -> Result<Self, p::Error> {
        Ok(Self {
            scope: Scope {
                account_id: queue.account().into(),
                key: DraftKey {
                    team: "T1".into(),
                    channel: "C00000001".into(),
                    thread: None,
                },
            },
            queue,
            case,
            reads: Mutex::new(vec![]),
            request: Mutex::new(None),
        })
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn caption(&self) -> &'static str {
        if self.case == HostedFixtureCase::FileOnly {
            ""
        } else {
            HOSTED_FIXTURE_CAPTION
        }
    }
    pub fn session(&self) -> WorkspaceSession {
        WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Disconnected hosted composer".into(),
            url: "https://fixture.slack.com".into(),
            token: "hosted-composer-disconnected-token".into(),
        }
    }
    pub fn file(id: &str) -> Option<Value> {
        let title = match id {
            "F00000001" => "Alpha 日本語.pdf",
            "F00000002" => "Beta e\u{301}.pdf",
            _ => return None,
        };
        Some(
            json!({"id":id,"mode":"hosted","is_external":false,"filetype":"pdf","title":title,"name":title,"mimetype":"application/pdf","size":7,"future":{"keep":[42,null,"源"]}}),
        )
    }
    pub fn channel() -> Value {
        json!({"id":"C00000001","name":"hosted-fixture","is_channel":true,"is_member":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false})
    }
    pub fn reads(&self) -> Vec<Value> {
        self.reads.lock().unwrap().clone()
    }
    pub fn request(&self) -> Option<Value> {
        self.request.lock().unwrap().clone()
    }
    /// The production builder supplies the actual request. Only these fixed
    /// read forms are answered, after full synthetic origin/token checks.
    pub fn read(&self, request: PreparedRequest) -> Result<Value, crate::slack::Error> {
        let fail = || crate::slack::Error::Api("hosted_fixture_read_refused".into());
        let RequestBody::Form(fields) = &request.body else {
            return Err(fail());
        };
        let url = url::Url::parse(&request.url).map_err(|_| fail())?;
        if request.method != "POST"
            || url.scheme() != "https"
            || url.host_str() != Some("fixture.slack.com")
            || url.username() != ""
            || url.password().is_some()
            || url.fragment().is_some()
            || url.port_or_known_default() != Some(443)
            || fields.first() != Some(&("token".into(), self.session().token))
        {
            return Err(fail());
        }
        let value = match url.path() {
            "/api/files.info" if fields.len() == 2 && fields[1].0 == "file" => {
                json!({"ok":true,"file":Self::file(&fields[1].1).ok_or_else(fail)?})
            }
            "/api/conversations.info"
                if fields
                    == &vec![
                        ("token".into(), self.session().token),
                        ("channel".into(), "C00000001".into()),
                        ("include_num_members".into(), "false".into()),
                    ] =>
            {
                json!({"ok":true,"channel":Self::channel()})
            }
            "/api/search.modules.files" => {
                let expected = vec![
                    ("token", "hosted-composer-disconnected-token"),
                    ("module", "files"),
                    (
                        "query",
                        "type:pdfs type:documents type:spreadsheets type:presentations type:images",
                    ),
                    ("page", "1"),
                    ("extracts", "1"),
                    ("highlight", "1"),
                    ("max_extract_len", "200"),
                    ("extra_message_data", "1"),
                    ("no_user_profile", "1"),
                    ("count", "50"),
                    ("file_title_only", "false"),
                    ("query_rewrite_disabled", "false"),
                    ("include_files_shares", "1"),
                    ("search_context", "desktop_files_browser"),
                    ("max_filter_suggestions", "10"),
                    ("sort", "last_engaged"),
                    ("sort_dir", "desc"),
                    ("search_exclude_bots", "false"),
                    ("search_only_my_channels", "false"),
                    ("shared_with_you", "0"),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect::<Vec<_>>();
                if fields != &expected {
                    return Err(fail());
                }
                json!({"ok":true,"items":[Self::file("F00000001").unwrap(),Self::file("F00000002").unwrap()],"pagination":{"page":1,"page_count":1}})
            }
            _ => return Err(fail()),
        };
        let mut reads = self.reads.lock().map_err(|_| fail())?;
        if reads.len() >= 32 {
            return Err(fail());
        }
        reads.push(
            json!({"method":request.method,"path":url.path(),"fields":fields,"response":value}),
        );
        Ok(value)
    }
    /// Consume the exact normally armed request from this same owned queue.
    /// The real native/account/source/policy fences run before this boundary.
    pub fn complete(&self, attempt: HostedAttempt) -> Result<(), HostedExecuteError> {
        let refuse = || HostedExecuteError::BeforeDispatch(AdmissionError::Stale);
        let input = attempt.completion.input();
        if !Arc::ptr_eq(&self.queue, &attempt.completion.queue)
            || input.review().scope() != &self.scope
        {
            return Err(refuse());
        }
        let expected_sources = vec![
            Self::file("F00000002").unwrap(),
            Self::file("F00000001").unwrap(),
        ];
        let blocks = if self.caption().is_empty() {
            vec![]
        } else {
            crate::drafts::codec::encode(&self.scope.key, self.caption())
                .map_err(|_| refuse())?
                .blocks
        };
        if input.input().sources() != expected_sources
            || input.input().snapshot().blocks != blocks
            || input.input().snapshot().file_ids != ["F00000002", "F00000001"]
        {
            return Err(refuse());
        }
        let mut fields = vec![
            ("token".into(), self.session().token),
            ("files".into(), "F00000002,F00000001".into()),
            ("channel".into(), "C00000001".into()),
        ];
        if !blocks.is_empty() {
            fields.push((
                "blocks".into(),
                serde_json::to_string(&blocks).map_err(|_| refuse())?,
            ));
        }
        fields.extend([
            ("resharing_aware".into(), "true".into()),
            ("skip_dlp_user_warning".into(), "false".into()),
            ("from_share_modal".into(), "false".into()),
        ]);
        if attempt.request.method != "POST"
            || attempt.request.url.split('?').next()
                != Some("https://fixture.slack.com/api/files.share")
            || attempt.request.body != RequestBody::Form(fields.clone())
        {
            return Err(refuse());
        }
        let mut held = self.request.lock().map_err(|_| refuse())?;
        if held.is_some() {
            return Err(refuse());
        }
        let returned = if self.case == HostedFixtureCase::Unknown {
            HostedReturned::Unknown
        } else {
            HostedReturned::Response(
                json!({"ok":true,"file_msg_ts":"1719800500.000001","future":{"keep":17}}),
            )
        };
        *held = Some(
            json!({"method":"POST","path":"/api/files.share","fields":fields,"returned":returned}),
        );
        attempt
            .completion
            .queue
            .worker
            .bind_hosted_returned(&attempt.completion.held, returned)
            .map_err(|r| HostedExecuteError::Returned(r.into()))?;
        attempt
            .completion
            .submit_returned()
            .map_err(HostedExecuteError::Storage)
    }
}
