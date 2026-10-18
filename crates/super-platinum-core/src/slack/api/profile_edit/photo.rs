//! Ordinary self photo flow captured from EditProfileUpload/Crop/RemovePane.
//! This is independent of text/profile-section writes and never accepts a user ID.
use super::*;
use std::sync::Arc;
mod image;
pub use image::{Image, MAX_BYTES, SquareCrop};
#[cfg(test)]
mod tests;

#[derive(Clone, PartialEq)]
pub struct Photo {
    fields: serde_json::Map<String, Value>,
}
impl std::fmt::Debug for Photo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Photo").finish_non_exhaustive()
    }
}
const KEYS: [&str; 10] = [
    "avatar_hash",
    "image_original",
    "image_24",
    "image_32",
    "image_48",
    "image_72",
    "image_192",
    "image_512",
    "image_1024",
    "is_custom_image",
];
impl Photo {
    pub fn from_profile(raw: &Value) -> Result<Self, Error> {
        let object = raw
            .as_object()
            .ok_or_else(|| malformed("Unreadable profile photo."))?;
        let mut fields = serde_json::Map::new();
        for key in KEYS {
            // UserProfile serializes absent optional image fields as null.
            // Both shapes denote an absent variant, not a later photo update.
            if let Some(value) = object.get(key).filter(|v| !v.is_null()) {
                if if key == "is_custom_image" {
                    !value.is_boolean()
                } else {
                    !value.is_string()
                } {
                    return Err(malformed("Unreadable profile photo field."));
                }
                fields.insert(key.into(), value.clone());
            }
        }
        // An entirely absent projection is not a definitive photo readback.
        if !fields
            .get("avatar_hash")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
            || !fields
                .get("image_48")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
        {
            return Err(malformed("Slack did not return a complete profile photo."));
        }
        Ok(Self { fields })
    }
    pub fn custom(&self) -> bool {
        self.fields
            .get("image_original")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
    }
    pub fn url(&self) -> &str {
        ["image_512", "image_192", "image_72", "image_48"]
            .into_iter()
            .find_map(|k| {
                self.fields
                    .get(k)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or("")
    }
    /// Only photo keys are replaced; status, title, names and unknown profile data survive.
    pub fn adopt(&self, target: &mut UserProfile) {
        let mut raw = serde_json::to_value(&*target).expect("profile serialization");
        let object = raw.as_object_mut().expect("profile object");
        for key in KEYS {
            object.remove(key);
        }
        object.extend(self.fields.clone());
        *target = serde_json::from_value(raw).expect("validated profile photo");
    }
}

#[derive(Clone)]
pub struct Capability {
    pub team: String,
    pub user: String,
    pub permission: ProfileEditCapability,
    pub photo: Photo,
}
pub fn permissions_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    boot: &Value,
) -> Result<PreparedRequest, Error> {
    let scope = BootScope::capture(w, boot)?;
    let mut args = json!({"user_id":w.user_id,"permissions":["EDIT_PROFILE_PHOTO"]});
    if scope.aggregate_workspaces() {
        args["team_ids"] = json!(scope.internal_teams);
    } else {
        args["as_admin"] = json!(true);
    }
    c.edge_json(w, "permissions/info", args)
        .map_err(|_| malformed("Could not prepare photo permissions."))
}
pub fn capability(
    w: &WorkspaceSession,
    boot: &Value,
    permissions: &Value,
    profile: &Value,
) -> Result<Capability, Error> {
    let scope = BootScope::capture(w, boot)?;
    success(permissions)?;
    let permission = match scope.permission(permissions, "EDIT_PROFILE_PHOTO") {
        ProfileEditCapability::Denied(_) => {
            ProfileEditCapability::Denied("Your organization does not allow changing your photo.")
        }
        permission => permission,
    };
    let photo = decode_own_photo(w, profile)?;
    Ok(Capability {
        team: w.team_id.clone(),
        user: w.user_id.clone(),
        permission,
        photo,
    })
}
pub async fn fetch_capability(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
) -> Result<Capability, Error> {
    let boot = t.execute(own_profile_boot_request(c, w)).await?;
    let request = permissions_request(c, w, &boot)?;
    let (permissions, profile) = tokio::try_join!(
        t.execute(request),
        t.execute(users_profile_get(c, w, w.user_id.clone()))
    )?;
    capability(w, &boot, &permissions, &profile)
}
pub async fn read(t: &Transport, c: &SlackClient, w: &WorkspaceSession) -> Result<Photo, Error> {
    let response = t
        .execute(users_profile_get(c, w, w.user_id.clone()))
        .await?;
    decode_own_photo(w, &response)
}
fn decode_own_photo(w: &WorkspaceSession, response: &Value) -> Result<Photo, Error> {
    success(response)?;
    if response
        .get("user_id")
        .is_some_and(|v| v.as_str() != Some(w.user_id.as_str()))
    {
        return Err(malformed("The profile response belongs to another member."));
    }
    Photo::from_profile(&response["profile"])
}

/// Closed upload request: file paths, arbitrary hosts/fields and other users are absent.
pub struct Upload {
    pub(crate) request: PreparedRequest,
    pub(crate) image: Image,
}
pub fn prepare(c: &SlackClient, w: &WorkspaceSession, image: Image) -> Upload {
    Upload {
        request: c.rest_form(
            w,
            "users.preparePhoto",
            vec![("_x_reason", "EditProfileModal".into())],
        ),
        image,
    }
}
#[derive(Clone)]
pub struct PreparedPhoto {
    id: String,
    url: String,
}
impl PreparedPhoto {
    pub fn parse(response: &Value) -> Result<Self, Error> {
        success(response)?;
        let id = response
            .get("id")
            .and_then(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .or_else(|| v.as_u64().filter(|n| *n > 0).map(|n| n.to_string()))
            })
            .filter(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
            .ok_or_else(|| malformed("Slack did not return a prepared photo identity."))?;
        let url = response
            .get("url")
            .and_then(Value::as_str)
            .filter(|s| trusted_image_url(s))
            .ok_or_else(|| malformed("Slack did not return a safe prepared photo URL."))?
            .to_owned();
        Ok(Self { id, url })
    }
    pub fn url(&self) -> &str {
        &self.url
    }
}
pub fn trusted_image_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "https"
            && u.username().is_empty()
            && u.password().is_none()
            && u.port_or_known_default() == Some(443)
            && u.fragment().is_none()
            && u.host_str().is_some_and(|h| {
                h == "slack.com"
                    || h.ends_with(".slack.com")
                    || h.ends_with(".slack-edge.com")
                    || h.ends_with(".slack-imgs.com")
            })
    })
}
pub struct Mutation {
    pub(crate) request: PreparedRequest,
}
pub fn set(
    c: &SlackClient,
    w: &WorkspaceSession,
    prepared: &PreparedPhoto,
    crop: SquareCrop,
) -> Mutation {
    Mutation {
        request: c.rest_form(
            w,
            "users.setPhoto",
            vec![
                ("id", prepared.id.clone()),
                ("crop_x", crop.x().to_string()),
                ("crop_y", crop.y().to_string()),
                ("crop_w", crop.size().to_string()),
                ("_x_reason", "edit-profile-set-photo".into()),
            ],
        ),
    }
}
pub fn remove(c: &SlackClient, w: &WorkspaceSession) -> Mutation {
    Mutation {
        request: c.rest_form(
            w,
            "users.deletePhoto",
            vec![("_x_reason", "edit_profile_delete_photo".into())],
        ),
    }
}
pub(crate) fn confirm(response: &Value) -> Result<(), Error> {
    success(response)
}
