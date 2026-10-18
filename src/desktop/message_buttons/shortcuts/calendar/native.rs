//! The final Create boundary owns a live DOM lease, not only queued input events.
use super::*;

#[derive(Clone, PartialEq, Eq, serde::Serialize)]
pub(super) struct Snapshot {
    fields: BTreeMap<String, Control>,
    #[serde(skip)]
    event: cal::Event,
    #[serde(skip)]
    provider: cal::Provider,
    #[serde(skip)]
    account: cal::Account,
    #[serde(skip)]
    time_revision: u64,
    #[serde(skip)]
    share_channel: String,
}
#[derive(Clone, PartialEq, Eq, serde::Serialize)]
struct Control {
    kind: &'static str,
    value: String,
    checked: Option<bool>,
}
impl Snapshot {
    pub(super) fn capture(s: &ShellState, id: u64) -> Option<Self> {
        let event = actions::event(s, id)?;
        let c = active(s, id)?;
        let mut fields = BTreeMap::new();
        let mut value = |id: &str, kind, text: String| {
            fields.insert(
                id.into(),
                Control {
                    kind,
                    value: text,
                    checked: None,
                },
            );
        };
        value("calendar-title", "text", c.title.clone());
        value("calendar-description", "textarea", c.description.clone());
        value("calendar-date", "date", c.date.clone());
        value("calendar-guest-query", "text", c.guest_query.clone());
        value("calendar-calendar", "select-one", c.calendar.clone());
        value("calendar-conference", "select-one", c.selected_room.clone());
        value(
            "calendar-mode",
            "select-one",
            if c.custom { "custom" } else { "suggested" }.into(),
        );
        if c.custom {
            value("calendar-start", "time", c.start_text.clone());
            value("calendar-end", "time", c.end_text.clone());
            // Manually typed times need not be one of the suggested options.
            let selected = c
                .start
                .filter(|v| c.slots.iter().any(|slot| slot.start == *v));
            value(
                "calendar-custom-slot",
                "select-one",
                selected.map(|v| v.to_string()).unwrap_or_default(),
            );
        } else {
            value("calendar-duration", "select-one", c.duration.to_string());
            value("calendar-slot", "select-one", c.start?.to_string());
        }
        if matches!(c.account, Some(cal::Account::Legacy { .. })) {
            value("calendar-share-query", "text", c.share_query.clone());
            // An unavailable retained destination has no option when sharing
            // is off. Capture the browser's empty selection, retaining the raw
            // draft separately for the exact post-await identity comparison.
            let selected = if !c.share
                && !c
                    .share_choices
                    .iter()
                    .any(|choice| choice.id == c.share_channel)
            {
                String::new()
            } else {
                c.share_channel.clone()
            };
            value("calendar-share-channel", "select-one", selected);
            fields.insert(
                "calendar-share".into(),
                Control {
                    kind: "checkbox",
                    value: "on".into(),
                    checked: Some(c.share),
                },
            );
        }
        Some(Self {
            fields,
            event,
            provider: c.provider,
            account: c.account.clone()?,
            time_revision: c.time_revision,
            share_channel: c.share_channel.clone(),
        })
    }
}

pub(super) struct Lease(dioxus::document::Eval);
impl Drop for Lease {
    fn drop(&mut self) {
        // Exact original nodes only. Also released locally when the dialog closes.
        let _ = self.0.send(());
    }
}
pub(super) async fn lock(id: u64, expected: &Snapshot) -> Option<Lease> {
    let expected = serde_json::to_string(expected).ok()?;
    let script = format!(
        "{}\nconst release=lockCalendarCreate({id},{expected});try{{dioxus.send(!!release);if(release)await dioxus.recv();}}finally{{release?.();}}",
        include_str!("native.js")
    );
    let mut eval = dioxus::document::eval(&script);
    if eval.recv::<bool>().await.ok() == Some(true) {
        Some(Lease(eval))
    } else {
        let _ = eval.send(());
        None
    }
}

#[cfg(test)]
mod tests;
