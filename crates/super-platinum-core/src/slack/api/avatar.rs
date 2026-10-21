//! Workflow avatar upload/crop, from MjoJ, 8dyy, Y0xf and CKzZ. These are
//! separate mutations before workflow metadata Save; no workflow is changed here.
use std::sync::Arc;

use serde_json::{Value, json};

use crate::config::WorkspaceSession;
use crate::slack::client::RequestBody;
use crate::slack::{Error, PreparedRequest, SlackClient};

mod image;
pub use image::{Image, MAX_BYTES};

pub const UPLOAD_REASON: &str = "workflows_upload_workflow_avatar";
pub const CROP_REASON: &str = "workflows_crop_workflow_avatar";

#[derive(Clone)]
pub struct AvatarRequest {
    pub(crate) request: PreparedRequest,
    pub(crate) image: Option<Image>,
}
impl std::fmt::Debug for AvatarRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AvatarRequest")
            .field("method", &self.method())
            .field("image", &self.image)
            .finish_non_exhaustive()
    }
}
impl AvatarRequest {
    /// Original validated image for owned byte comparison or media registration.
    /// Do not log its bytes; request inspection and Debug remain redacted.
    pub fn image(&self) -> Option<&Image> {
        self.image.as_ref()
    }

    pub fn method(&self) -> &'static str {
        if self.image.is_some() {
            "avatars.upload"
        } else {
            "avatars.crop"
        }
    }
    /// Safe closed-fixture inspection: no token, cookie, image bytes or request
    /// headers. The URL contains only the existing client's transport metadata.
    pub fn inspect(&self) -> Value {
        let RequestBody::Form(fields) = &self.request.body else {
            unreachable!("closed factory")
        };
        json!({"method":self.method(),"url":self.request.url,
            "fields":fields.iter().filter(|(key,_)|key!="token").collect::<Vec<_>>(),
            "tokenPresent":fields.first().is_some_and(|(k,v)|k=="token"&&!v.is_empty()),
            "image":self.image.as_ref().map(|i|json!({"filename":i.filename(),"mime":i.mime(),"length":i.bytes.len(),"width":i.width(),"height":i.height()}))})
    }
}

pub fn upload(client: &SlackClient, session: &WorkspaceSession, image: Image) -> AvatarRequest {
    AvatarRequest {
        request: client.avatar_multipart(
            session,
            "avatars.upload",
            vec![("_x_reason", UPLOAD_REASON.into())],
        ),
        image: Some(image),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crop {
    x: u32,
    y: u32,
    size: u32,
}
impl Crop {
    /// Full-image crop coordinates, not display pixels. Source rounds each
    /// nonnegative coordinate using Math.round. Never clamp an invalid selection.
    pub fn new(x: f64, y: f64, size: f64, width: u32, height: u32) -> Result<Self, Error> {
        if ![x, y, size].into_iter().all(f64::is_finite)
            || x < 0.0
            || y < 0.0
            || !(128.0..=2000.0).contains(&size)
            || !(128..=2000).contains(&width)
            || !(128..=2000).contains(&height)
            || x + size > f64::from(width)
            || y + size > f64::from(height)
        {
            return Err(Error::Api("avatar_invalid_crop".into()));
        }
        let crop = Self {
            x: (x + 0.5).floor() as u32,
            y: (y + 0.5).floor() as u32,
            size: (size + 0.5).floor() as u32,
        };
        if crop.x + crop.size > width || crop.y + crop.size > height {
            return Err(Error::Api("avatar_invalid_crop".into()));
        }
        Ok(crop)
    }
    pub fn cropbox(self) -> String {
        format!("{},{},{}", self.x, self.y, self.size)
    }
}

pub fn crop(
    client: &SlackClient,
    session: &WorkspaceSession,
    avatar: &str,
    crop: Crop,
) -> Result<AvatarRequest, Error> {
    if avatar.trim().is_empty() || avatar.chars().any(char::is_control) {
        return Err(Error::Api("avatar_invalid_id".into()));
    }
    Ok(AvatarRequest {
        request: client.avatar_multipart(
            session,
            "avatars.crop",
            vec![
                ("avatar", avatar.into()),
                ("cropbox", crop.cropbox()),
                ("_x_reason", CROP_REASON.into()),
            ],
        ),
        image: None,
    })
}

/// A usable actual upload response, not an invented server schema. Unknown
/// fields are retained; an unusable successful response is an Unknown outcome.
#[derive(Clone)]
pub struct UploadReply {
    raw: Value,
    id: String,
    url: String,
}
#[derive(Clone)]
pub struct CropReply {
    raw: Value,
    url: String,
}
fn response_url(raw: &Value) -> Result<String, Error> {
    if raw.get("ok") != Some(&Value::Bool(true)) || raw.get("error").is_some_and(|v| !v.is_null()) {
        return Err(Error::Api("avatar_response_not_acknowledged".into()));
    }
    let value = raw
        .get("url")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| Error::Transport("Avatar response has no usable URL".into()))?;
    // Response hosts are not inferred. Native media admission owns download
    // origin policy; reject non-HTTPS/credential-bearing URLs before projection.
    let parsed = url::Url::parse(value)
        .map_err(|_| Error::Transport("Avatar response URL is invalid".into()))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(Error::Transport("Avatar response URL is invalid".into()));
    }
    Ok(value.into())
}
impl UploadReply {
    pub fn parse(raw: &Value) -> Result<Self, Error> {
        let url = response_url(raw)?;
        let id = raw
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty() && !s.chars().any(char::is_control))
            .ok_or_else(|| Error::Transport("Avatar upload response has no usable ID".into()))?;
        Ok(Self {
            raw: raw.clone(),
            id: id.into(),
            url,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}
impl CropReply {
    pub fn parse(raw: &Value) -> Result<Self, Error> {
        Ok(Self {
            raw: raw.clone(),
            url: response_url(raw)?,
        })
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}
impl std::fmt::Debug for UploadReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadReply").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for CropReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CropReply").finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Acknowledged,
    Rejected,
    Unknown,
    Retired,
}
/// Observed once for each dispatched attempt, including retired in-flight
/// attempts. Raw bodies are retained but intentionally excluded from Debug.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub number: u8,
    pub status: Option<u16>,
    pub outcome: Outcome,
    pub raw: Option<Value>,
    pub response_text: Option<String>,
    pub retry_after_millis: Option<u64>,
    pub may_have_applied: bool,
}
impl std::fmt::Debug for Attempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AvatarAttempt")
            .field("number", &self.number)
            .field("status", &self.status)
            .field("outcome", &self.outcome)
            .field("may_have_applied", &self.may_have_applied)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
