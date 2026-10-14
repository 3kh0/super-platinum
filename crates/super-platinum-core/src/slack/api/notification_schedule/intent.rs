use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduleAction {
    Mode(ScheduleMode),
    Time {
        day: Option<ScheduleDay>,
        bound: ScheduleBound,
        value: String,
    },
    None(ScheduleDay),
}
#[derive(Clone)]
pub struct NotificationScheduleIntent {
    team: String,
    user: String,
    baseline: NotificationSchedule,
    patch: BTreeMap<String, Value>,
    keys: Vec<String>,
}
impl NotificationScheduleIntent {
    pub fn capture(
        w: &WorkspaceSession,
        baseline: &NotificationSchedule,
        action: ScheduleAction,
    ) -> Result<Self, Error> {
        let view = baseline.view()?;
        if baseline
            .timezone
            .as_ref()
            .is_none_or(|(user, _)| user != &w.user_id)
        {
            return Err(Error::Api(
                "notification_schedule_timezone_unavailable".into(),
            ));
        }
        let mut patch = BTreeMap::new();
        match action {
            ScheduleAction::Mode(mode) => {
                patch.insert("dnd_days".into(), json!(mode.as_str()));
            }
            ScheduleAction::None(day) => {
                if view.mode != Some(ScheduleMode::Custom)
                    || !view.days.iter().any(|d| d.day == day && d.can_none)
                {
                    return Err(rejected());
                }
                patch.insert(format!("dnd_enabled_{}", day.as_str()), json!("all_day"));
            }
            ScheduleAction::Time { day, bound, value } => {
                if day.is_some() != (view.mode == Some(ScheduleMode::Custom))
                    || value == "all_day"
                    || !baseline.options(day, bound)?.contains(&value)
                {
                    return Err(rejected());
                }
                let (key, paired) = match (day, bound) {
                    (None, ScheduleBound::Start) => {
                        ("dnd_end_hour".into(), Some("dnd_start_hour".into()))
                    }
                    (None, ScheduleBound::End) => ("dnd_start_hour".into(), None),
                    (Some(d), ScheduleBound::Start) => (
                        format!("dnd_before_{}", d.as_str()),
                        Some(format!("dnd_after_{}", d.as_str())),
                    ),
                    (Some(d), ScheduleBound::End) => (format!("dnd_after_{}", d.as_str()), None),
                };
                if let Some(d) = day {
                    if bound == ScheduleBound::End && view.days.iter().any(|v| v.day == d && v.none)
                    {
                        return Err(rejected());
                    }
                    patch.insert(format!("dnd_enabled_{}", d.as_str()), json!("partial"));
                }
                if let Some(paired) = paired {
                    // DndPreferences checks original props, even when a disabled
                    // schedule/day currently displays full-day preview hours.
                    let original = baseline.value(&paired);
                    let compare = match original {
                        Some(Value::String(s)) => format!("{:0>5}", s),
                        None => "00000".into(),
                        Some(Value::Null) => "00000".into(),
                        _ => return Err(invalid()),
                    };
                    if value >= compare {
                        patch.insert(paired, json!(presentation::next_time(&value)));
                    }
                }
                patch.insert(key, json!(value));
            }
        }
        patch.insert("dnd_enabled".into(), json!(true));
        let keys = patch.keys().cloned().collect();
        Ok(Self {
            team: w.team_id.clone(),
            user: w.user_id.clone(),
            baseline: baseline.clone(),
            patch,
            keys,
        })
    }
    pub fn team_id(&self) -> &str {
        &self.team
    }
    pub fn user_id(&self) -> &str {
        &self.user
    }
    pub fn desired_patch(&self) -> &BTreeMap<String, Value> {
        &self.patch
    }
    pub fn changed_keys(&self) -> &[String] {
        &self.keys
    }
    pub fn revalidate(&self, fresh: &NotificationSchedule) -> Result<(), Error> {
        // time24 affects labels only, never the canonical allowed-hour choice.
        let same = self
            .baseline
            .raw
            .iter()
            .filter(|(k, _)| k.as_str() != "time24")
            .eq(fresh.raw.iter().filter(|(k, _)| k.as_str() != "time24"));
        if same && self.baseline.timezone == fresh.timezone {
            Ok(())
        } else {
            Err(Error::Api("notification_schedule_changed".into()))
        }
    }
    pub fn confirms(&self, fresh: &NotificationSchedule) -> bool {
        fresh
            .timezone
            .as_ref()
            .is_some_and(|(user, _)| user == &self.user)
            && self.patch.iter().all(|(k, v)| fresh.raw.get(k) == Some(v))
    }
}
pub fn notification_schedule_update_request(
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &NotificationScheduleIntent,
    fresh: &NotificationSchedule,
) -> Result<PreparedRequest, Error> {
    if w.team_id != intent.team || w.user_id != intent.user {
        return Err(Error::Api("notification_schedule_owner_changed".into()));
    }
    intent.revalidate(fresh)?;
    Ok(c.rest_form(
        w,
        "users.prefs.set",
        vec![
            (
                "prefs",
                serde_json::to_string(&intent.patch).map_err(|_| rejected())?,
            ),
            ("_x_reason", "prefs-api/setMultipleUserPrefsByApi".into()),
        ],
    ))
}
pub async fn set_notification_schedule(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    intent: &NotificationScheduleIntent,
    fresh: &NotificationSchedule,
) -> Result<(), Error> {
    t.execute_notification_schedule(notification_schedule_update_request(c, w, intent, fresh)?)
        .await?;
    Ok(())
}
