//! Separate new-DM fixture seam. Existing Share evidence schema remains unchanged.
use super::*;
impl Capability {
    pub(in crate::file_share) async fn direct_lookup(
        &self,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        let expected = super_platinum_core::slack::api::file_share_im_list(
            &SlackClient::default(),
            &self.builder_session(),
        );
        if request.method != expected.method
            || request.url.split('?').next() != expected.url.split('?').next()
            || request.headers != expected.headers
            || request.body != expected.body
        {
            return Err(rejected(Rejected::Request));
        }
        self.read("im.list").map_err(rejected)?;
        let opened = self
            .0
            .ledger
            .lock()
            .map_err(|_| rejected(Rejected::Poisoned))?
            .direct_open
            .is_some();
        Ok(json!({"ok":true,"ims":if opened {vec![self.0.conversation.raw()]}else{vec![]}}))
    }

    pub(in crate::file_share) async fn open_direct(
        &self,
        request: PreparedRequest,
    ) -> Result<Value, Error> {
        if self.0.conversation != Conversation::NewDirect || self.0.restart.is_some() {
            return Err(rejected(Rejected::Request));
        }
        let recipients =
            super_platinum_core::direct_messages::Recipients::new(&["U00000002".into()], USER)
                .map_err(|_| rejected(Rejected::Request))?;
        let expected = super_platinum_core::slack::api::conversations_open_recipients(
            &SlackClient::default(),
            &self.builder_session(),
            &recipients,
        );
        let (RequestBody::Form(fields), RequestBody::Form(wanted)) =
            (&request.body, &expected.body)
        else {
            return Err(rejected(Rejected::Request));
        };
        if !matches_open(&request, &expected) || fields != wanted {
            return Err(rejected(Rejected::Request));
        }
        let mut ledger = self
            .0
            .ledger
            .lock()
            .map_err(|_| rejected(Rejected::Poisoned))?;
        if ledger.direct_open.is_some() || ledger.attempt.is_some() || ledger.begun.is_some() {
            return Err(rejected(Rejected::Replay));
        }
        ledger.direct_open = Some(fields.clone());
        Ok(json!({"ok":true,"channel":{"id":"D00000001"},"future":{"preserve":true}}))
    }
    pub(in crate::file_share) fn direct_evidence(&self) -> Result<Value, Rejected> {
        let ledger = self.0.ledger.lock().map_err(|_| Rejected::Poisoned)?;
        Ok(json!({"count":usize::from(ledger.direct_open.is_some()),"fields":ledger.direct_open}))
    }
}

fn matches_open(actual: &PreparedRequest, expected: &PreparedRequest) -> bool {
    if actual.method != expected.method
        || actual.headers != expected.headers
        || actual.body != expected.body
    {
        return false;
    }
    let Some((actual_base, actual_query)) = actual.url.split_once('?') else {
        return false;
    };
    let Some((expected_base, expected_query)) = expected.url.split_once('?') else {
        return false;
    };
    if expected_base != format!("{ORIGIN}/api/conversations.open") || actual_base != expected_base {
        return false;
    }
    let actual: Vec<_> = actual_query.split('&').collect();
    let expected: Vec<_> = expected_query.split('&').collect();
    if actual.len() != expected.len() {
        return false;
    }
    let mut generated = Vec::new();
    for (actual, expected) in actual.iter().zip(&expected) {
        let Some((key, value)) = actual.split_once('=') else {
            return false;
        };
        let Some((expected_key, expected_value)) = expected.split_once('=') else {
            return false;
        };
        if key != expected_key {
            return false;
        }
        if matches!(key, "_x_id" | "_x_csid" | "fp") {
            if !simple_v4(value) || !simple_v4(expected_value) || generated.contains(&value) {
                return false;
            }
            generated.push(value);
        } else if value != expected_value {
            return false;
        }
    }
    generated.len() == 3
}

fn simple_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 32
        && bytes
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        && bytes[12] == b'4'
        && matches!(bytes[16], b'8' | b'9' | b'a' | b'b')
}
