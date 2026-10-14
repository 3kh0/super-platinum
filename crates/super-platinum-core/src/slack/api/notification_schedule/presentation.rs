use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationScheduleView {
    pub enabled: bool,
    pub mode: Option<ScheduleMode>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub days: Vec<ScheduleDayView>,
    pub time24: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleDayView {
    pub day: ScheduleDay,
    pub none: bool,
    pub can_none: bool,
    pub start: Option<String>,
    pub end: Option<String>,
}
fn flag(value: Option<&Value>) -> Result<bool, Error> {
    match value {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(v)) => Ok(*v),
        _ => Err(invalid()),
    }
}
/// Source padStart(5,"0") accepts canonical one-digit hours without rounding.
pub(super) fn time(value: Option<&Value>) -> Result<Option<String>, Error> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) => {
            let (h, m) = s.split_once(':').ok_or_else(invalid)?;
            if !(1..=2).contains(&h.len())
                || m.len() != 2
                || !h.bytes().chain(m.bytes()).all(|b| b.is_ascii_digit())
            {
                return Err(invalid());
            }
            let hour: u8 = h.parse().map_err(|_| invalid())?;
            let minute: u8 = m.parse().map_err(|_| invalid())?;
            if hour > 23 || minute > 59 {
                return Err(invalid());
            }
            Ok(Some(format!("{hour:02}:{minute:02}")))
        }
        _ => Err(invalid()),
    }
}
impl NotificationSchedule {
    pub fn view(&self) -> Result<NotificationScheduleView, Error> {
        let enabled = flag(self.value("dnd_enabled"))?;
        let mode = match self.value("dnd_days") {
            None | Some(Value::Null) => None,
            Some(Value::String(v)) => match v.as_str() {
                "" => None,
                "every_day" => Some(ScheduleMode::EveryDay),
                "weekdays" => Some(ScheduleMode::Weekdays),
                "custom" => Some(ScheduleMode::Custom),
                _ => return Err(invalid()),
            },
            _ => return Err(invalid()),
        };
        let count = ScheduleDay::ALL
            .iter()
            .filter(|day| {
                self.value(&format!("dnd_enabled_{}", day.as_str()))
                    .and_then(Value::as_str)
                    == Some("all_day")
            })
            .count();
        let mut days = Vec::new();
        for day in ScheduleDay::ALL {
            let key = day.as_str();
            let state = match self.value(&format!("dnd_enabled_{key}")) {
                None | Some(Value::Null) => "",
                Some(Value::String(s))
                    if matches!(s.as_str(), "" | "all_day" | "disabled" | "partial") =>
                {
                    s
                }
                _ => return Err(invalid()),
            };
            let start = time(self.value(&format!("dnd_before_{key}")))?;
            let end = time(self.value(&format!("dnd_after_{key}")))?;
            days.push(ScheduleDayView {
                day,
                none: state == "all_day",
                can_none: state == "all_day" || count != 6,
                start: if state == "disabled" {
                    Some("00:00".into())
                } else {
                    start
                },
                end: if state == "disabled" {
                    Some("23:59".into())
                } else {
                    end
                },
            });
        }
        let start = time(self.value("dnd_end_hour"))?;
        let end = time(self.value("dnd_start_hour"))?;
        Ok(NotificationScheduleView {
            enabled,
            mode: if enabled {
                mode
            } else {
                Some(ScheduleMode::EveryDay)
            },
            start: if enabled { start } else { Some("00:00".into()) },
            end: if enabled { end } else { Some("23:59".into()) },
            days,
            time24: flag(self.value("time24"))?,
        })
    }
    pub fn options(
        &self,
        day: Option<ScheduleDay>,
        bound: ScheduleBound,
    ) -> Result<Vec<String>, Error> {
        let view = self.view()?;
        let (start, end, none) = if let Some(day) = day {
            let d = view.days.iter().find(|d| d.day == day).unwrap();
            (d.start.as_deref(), d.end.as_deref(), d.can_none)
        } else {
            (view.start.as_deref(), view.end.as_deref(), false)
        };
        let base = half_hours();
        let mut choices = match bound {
            ScheduleBound::Start => {
                let mut v = base;
                if none {
                    v.insert(0, "all_day".into());
                }
                v
            }
            ScheduleBound::End => {
                let start = start.unwrap_or("00000");
                let mut all = base;
                all.push("23:59".into());
                let mut v: Vec<_> = all.iter().filter(|v| v.as_str() > start).cloned().collect();
                if day.is_none() {
                    v.extend(
                        all.into_iter()
                            .filter(|v| v.as_str() < start && v != "00:00"),
                    );
                }
                v
            }
        };
        let retained = match bound {
            ScheduleBound::Start => start,
            ScheduleBound::End => end,
        };
        if let Some(value) = retained {
            if !choices.iter().any(|v| v == value) {
                // JiU7 inserts an irregular canonical value into the cyclic order.
                let at = if choices.first().is_some_and(|v| value < v.as_str()) {
                    choices
                        .iter()
                        .rposition(|v| v.as_str() < value)
                        .map_or(0, |i| i + 1)
                } else {
                    choices
                        .iter()
                        .position(|v| v.as_str() > value)
                        .unwrap_or(choices.len().saturating_sub(1))
                };
                choices.insert(at, value.into());
            }
        }
        Ok(choices)
    }
}
pub(super) fn half_hours() -> Vec<String> {
    (0..48)
        .map(|n| format!("{:02}:{:02}", n / 2, (n % 2) * 30))
        .collect()
}
pub(super) fn next_time(value: &str) -> String {
    half_hours()
        .into_iter()
        .find(|t| t.as_str() > value)
        .unwrap_or_else(|| "23:59".into())
}
