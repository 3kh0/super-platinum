use super::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub real_name: Option<String>,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub is_owner: bool,
    #[serde(default)]
    pub is_restricted: bool,
    #[serde(default)]
    pub is_ultra_restricted: bool,
    #[serde(default)]
    pub is_stranger: bool,
    #[serde(default)]
    pub tz: Option<String>,
    #[serde(default)]
    pub tz_label: Option<String>,
    #[serde(default)]
    pub tz_offset: Option<i32>,
    #[serde(default)]
    pub profile: Option<UserProfile>,
    #[serde(default)]
    pub im_mpim_ids: Vec<ChannelId>,
    #[serde(default)]
    pub has_more_mpims: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserProfile {
    #[serde(default)]
    pub real_name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub pronouns: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub image_24: Option<String>,
    #[serde(default)]
    pub image_32: Option<String>,
    #[serde(default)]
    pub image_48: Option<String>,
    #[serde(default)]
    pub image_72: Option<String>,
    #[serde(default)]
    pub image_192: Option<String>,
    #[serde(default)]
    pub image_512: Option<String>,
    #[serde(default)]
    pub image_original: Option<String>,
    #[serde(default)]
    pub avatar_hash: Option<String>,
    #[serde(default)]
    pub team: Option<String>,
    #[serde(default)]
    pub status_text: Option<String>,
    #[serde(default)]
    pub status_emoji: Option<String>,
    #[serde(default)]
    pub status_expiration: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub fields: BTreeMap<String, ProfileFieldValue>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileFieldValue {
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub alt: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserProfilePage {
    #[serde(default)]
    pub profile: UserProfile,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileExtrasPage {
    #[serde(default)]
    pub im_mpim_ids: Vec<ChannelId>,
    #[serde(default)]
    pub has_more_mpims: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamProfilePage {
    #[serde(default)]
    pub profile: TeamProfile,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamProfile {
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub fields: Vec<TeamProfileField>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamProfileField {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(rename = "type", default)]
    pub field_type: Option<String>,
    #[serde(default)]
    pub ordering: Option<i64>,
    #[serde(default)]
    pub is_hidden: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// `dnd.info` for the signed-in user: the manual snooze plus the scheduled
/// Do Not Disturb window. Slack reports both independently, and the rail badge
/// only cares whether either one is muting notifications right now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DndInfo {
    #[serde(default)]
    pub dnd_enabled: bool,
    #[serde(default)]
    pub next_dnd_start_ts: Option<i64>,
    #[serde(default)]
    pub next_dnd_end_ts: Option<i64>,
    #[serde(default)]
    pub snooze_enabled: bool,
    #[serde(default)]
    pub snooze_endtime: Option<i64>,
    #[serde(default)]
    pub snooze_remaining: Option<i64>,
}

impl DndInfo {
    /// True while notifications are actually suppressed. `dnd_enabled` alone is
    /// not enough: the scheduled window is reported even when it is still in
    /// the future, so it only counts once `now` is inside it.
    pub fn is_snoozed(&self, now: i64) -> bool {
        if self.snooze_enabled && self.snooze_endtime.is_none_or(|end| end > now) {
            return true;
        }
        self.dnd_enabled
            && self.next_dnd_start_ts.is_some_and(|start| start <= now)
            && self.next_dnd_end_ts.is_some_and(|end| end > now)
    }
}
