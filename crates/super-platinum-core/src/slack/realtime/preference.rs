//! Original accepted preference bytes, beside unchanged synthetic RtEvent APIs.
use super::*;
use std::sync::Arc;
#[derive(Clone)]
pub struct PreferenceCarrier {
    original: Arc<str>,
    event: RtEvent,
}
impl std::fmt::Debug for PreferenceCarrier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreferenceCarrier(..)")
    }
}
impl PreferenceCarrier {
    /// Parsing retains bytes; it supplies no socket/account authority. Consumers
    /// must pair the original supervisor generation with its accepted owner.
    pub fn parse(original: Arc<str>) -> Option<Self> {
        if original.len() > 64 * 1024 {
            return None;
        }
        let event = parse_event(&original)?;
        matches!(event, RtEvent::PreferencesChanged { .. }).then_some(Self { original, event })
    }
    pub fn event(&self) -> &RtEvent {
        &self.event
    }
    pub fn original(&self) -> Arc<str> {
        self.original.clone()
    }
}
pub(super) fn update(generation: u64, event: RtEvent, original: &str) -> RtUpdate {
    if matches!(event, RtEvent::PreferencesChanged { .. }) {
        if let Some(carrier) = PreferenceCarrier::parse(original.into()) {
            return RtUpdate::Preference {
                generation,
                carrier,
            };
        }
    }
    RtUpdate::Event {
        generation,
        event: Box::new(event),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_socket_preference_bytes_survive_without_changing_synthetic_event_parser() {
        let raw = "{ \"type\":\"pref_change\", \"value\":false,\"name\":\"search_exclude_bots\",\"extra\":17 }";
        let event = parse_event(raw).unwrap();
        let RtUpdate::Preference {
            generation,
            carrier,
        } = update(9, event, raw)
        else {
            panic!("original preference carrier")
        };
        assert_eq!(generation, 9);
        assert_eq!(&*carrier.original(), raw);
        assert!(
            matches!(carrier.event(),RtEvent::PreferencesChanged{name,value}if name=="search_exclude_bots"&&value==false)
        );
        assert!(PreferenceCarrier::parse("{\"type\":\"hello\"}".into()).is_none());
        assert!(!format!("{carrier:?}").contains("search_exclude_bots"));
        assert!(
            matches!(parse_event(raw), Some(RtEvent::PreferencesChanged { .. })),
            "original parser API stays unchanged"
        );
    }
}
