//! Fixed original-client CRDT seeds; independent canonical lease metadata only.
//! Provenance: tmp/canvas-end-split-20261023/reference/parent-seeds.json.
use super::*;
pub(super) fn wire(corpus: &str) -> RawMessage {
    let seeds: Value = serde_json::from_str(include_str!("end-original.json")).unwrap();
    let bytes = STANDARD.decode(seeds[corpus].as_str().unwrap()).unwrap();
    RawMessage::decode(&bytes)
        .unwrap()
        .replace_fields(4, &[n(4, 8_000_000_000_000_000)])
        .unwrap()
}
