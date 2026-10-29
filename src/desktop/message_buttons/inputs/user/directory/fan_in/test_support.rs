use super::*;
use serde_json::json;
use std::cell::RefCell;
use tokio::sync::oneshot;

pub(crate) struct Scenario {
    case: u8,
    query: String,
    users: Cell<usize>,
    channels: Cell<usize>,
    user_receive: RefCell<Option<oneshot::Receiver<Result<Value, Error>>>>,
    channel_receive: RefCell<Option<oneshot::Receiver<Result<Value, Error>>>>,
    user_send: RefCell<Option<oneshot::Sender<Result<Value, Error>>>>,
    channel_send: RefCell<Option<oneshot::Sender<Result<Value, Error>>>>,
}
impl Scenario {
    pub(crate) fn new(case: u8, query: String) -> Rc<Self> {
        let (user_send, user_receive) = oneshot::channel();
        let (channel_send, channel_receive) = oneshot::channel();
        Rc::new(Self {
            case,
            query,
            users: Cell::new(0),
            channels: Cell::new(0),
            user_receive: RefCell::new(Some(user_receive)),
            channel_receive: RefCell::new(Some(channel_receive)),
            user_send: RefCell::new(Some(user_send)),
            channel_send: RefCell::new(Some(channel_send)),
        })
    }
    pub(crate) fn counts(&self) -> (usize, usize) {
        (self.users.get(), self.channels.get())
    }
    pub(crate) async fn execute(
        &self,
        provider: Provider,
        request: PreparedRequest,
        scope: ReadScope,
    ) -> Result<Option<Value>, Error> {
        if !scope.admitted() {
            return Ok(None);
        }
        let calls = if provider == Provider::Users {
            &self.users
        } else {
            &self.channels
        };
        let call = calls.get();
        calls.set(call + 1);
        assert!(request.url.contains(&format!("/{}/search", provider.key())));
        let super_platinum_core::slack::client::RequestBody::Json(body) = request.body else {
            panic!("JSON")
        };
        assert_eq!(body["count"], 24);
        assert_eq!(body["query"], self.query);
        assert_eq!(body["fuzz"], 1);
        assert!(body.get("external_team").is_none());
        assert!(body.get("allowed_internal_teams").is_none());
        if provider == Provider::Users {
            assert_eq!(body["include_profile_only_users"], true);
            assert_eq!(body["filter"], "NOT deactivated");
        } else {
            assert_eq!(body["check_membership"], true);
            assert_eq!(body["include_record_channels"], true);
        }
        if call == 0 {
            assert!(body.get("marker").is_none());
        } else {
            assert_eq!(body["marker"], format!("{}-next", provider.key()));
        }
        if self.case > 0 {
            assert_eq!(call, 0, "known quota must refuse every successor");
            let receiver = if provider == Provider::Users {
                self.user_receive.borrow_mut().take().unwrap()
            } else {
                self.channel_receive.borrow_mut().take().unwrap()
            };
            return scope
                .finish_injected(async { receiver.await.unwrap() })
                .await;
        }
        scope.finish_injected(async{Ok(if call==0 {
            json!({"results":vec![Value::Null;if provider==Provider::Users {20}else{0}],"next_marker":format!("{}-next",provider.key())})
        } else if provider==Provider::Users {
            json!({"results":(20000000..20000020).map(|n|json!({"id":format!("U{n:08}"),"name":format!("later-user-{n}"),"profile":{"display_name":format!("later-user-{n}")}})).collect::<Vec<_>>(),"next_marker":{"unused":true}})
        } else {json!({"results":[{"id":"C20000000","name":"later-channel","is_channel":true}]})})}).await
    }
    pub(crate) fn release_user(&self) {
        self.user_send
            .borrow_mut()
            .take()
            .unwrap()
            .send(Err(Error::RateLimited {
                retry_after_secs: Some(if self.case == 2 { 12 } else { 37 }),
            }))
            .unwrap();
    }
    pub(crate) fn release_channel(&self) {
        self.channel_send
            .borrow_mut()
            .take()
            .unwrap()
            .send(if self.case == 2 {
                Err(Error::RateLimited {
                    retry_after_secs: Some(37),
                })
            } else {
                Ok(json!({"results":[],"next_marker":"must-not-dispatch"}))
            })
            .unwrap();
    }
}
