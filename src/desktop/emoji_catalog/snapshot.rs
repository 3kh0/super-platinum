//! Only the exact disconnected diagnostic can choose packed capture.
use super::{CustomRecord, probe};
use std::collections::HashMap;
use super_platinum_core::slack::models::Emoji;
#[path = "diagnostic_arm.rs"]
mod arm;
#[path = "packed.rs"]
mod packed;

pub(super) enum Snapshot {
    Current(Vec<CustomRecord>),
    Packed(packed::Packed),
}

pub(super) fn capture(source: &HashMap<String, Emoji>) -> Result<Snapshot, String> {
    capture_mode(source, arm::PACKED && probe::enabled())
}

fn capture_mode(source: &HashMap<String, Emoji>, use_packed: bool) -> Result<Snapshot, String> {
    if use_packed {
        packed::Packed::capture(source.values().map(|e| (e.name.as_str(), e.value.as_str())))
            .map(Snapshot::Packed)
            .map_err(|error| format!("Packed capture: {error:?}"))
    } else {
        Ok(Snapshot::Current(
            source.values().map(CustomRecord::from).collect(),
        ))
    }
}

impl Snapshot {
    pub(super) fn mode(&self) -> &'static str {
        match self {
            Self::Current(_) => "current",
            Self::Packed(_) => "packed",
        }
    }
    pub(super) fn decode(self) -> Result<(Vec<CustomRecord>, Option<packed::Packed>), String> {
        match self {
            Self::Current(records) => Ok((records, None)),
            Self::Packed(buffer) => buffer
                .decode()
                .map(|records| (records, Some(buffer)))
                .map_err(|error| format!("Packed decode: {error:?}")),
        }
    }
}

pub(super) fn build_local(
    snapshot: Snapshot,
    ticket: u64,
    revision: u64,
) -> Result<
    (
        std::sync::Arc<super::CatalogPayload>,
        std::sync::Arc<super::PickerPayload>,
    ),
    String,
> {
    if !probe::enabled() {
        let Snapshot::Current(records) = snapshot else {
            unreachable!("packed snapshot outside exact diagnostic")
        };
        return super::build(records);
    }
    let mode = snapshot.mode();
    let total_at = probe::begin();
    let result = (|| {
        let decode_at = probe::begin();
        let decoded = snapshot.decode();
        probe::finish(
            decode_at,
            "snapshot-decode",
            || serde_json::json!({"ticket":ticket,"revision":revision,"mode":mode,"ok":decoded.is_ok()}),
        );
        let (records, retained) = decoded?;
        let release_at = probe::begin();
        drop(retained);
        probe::finish(
            release_at,
            "snapshot-buffer-release",
            || serde_json::json!({"ticket":ticket,"revision":revision,"mode":mode}),
        );
        let build_at = probe::begin();
        let result = super::build(records);
        probe::finish(
            build_at,
            "build-and-picker",
            || serde_json::json!({"ticket":ticket,"revision":revision,"ok":result.is_ok()}),
        );
        result
    })();
    probe::finish(
        total_at,
        "local-worker-total",
        || serde_json::json!({"ticket":ticket,"revision":revision,"mode":mode,"ok":result.is_ok()}),
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_snapshot_representations_preserve_order_and_actual_picker() {
        let source: HashMap<_, _> = [
            ("rocket", "https://example.invalid/rocket.png"),
            ("linked", "alias:rocket"),
            ("cycle", "alias:cycle"),
            ("bad:name", "https://example.invalid/bad.png"),
            ("empty", ""),
        ]
        .into_iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                Emoji {
                    name: name.into(),
                    value: value.into(),
                    ..Default::default()
                },
            )
        })
        .collect();
        let (direct, none) = capture_mode(&source, false).unwrap().decode().unwrap();
        assert!(none.is_none());
        let (decoded, buffer) = capture_mode(&source, true).unwrap().decode().unwrap();
        assert!(buffer.is_some());
        assert_eq!(direct, decoded);
        drop(buffer);
        let (a, pa) = super::super::build(direct).unwrap();
        let (b, pb) = super::super::build(decoded).unwrap();
        assert_eq!(a.names, b.names);
        assert_eq!(pa.len(), pb.len());
        for (a, b) in pa.iter().zip(pb.iter()) {
            assert_eq!(
                (&a.name, &a.glyph, &a.url, &a.aliases),
                (&b.name, &b.glyph, &b.url, &b.aliases)
            );
        }
    }
}
