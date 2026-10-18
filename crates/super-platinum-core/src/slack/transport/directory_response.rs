//! Same-byte raw response and selected ECMAScript object-key encounter order.
use super::*;
mod decode;

/// Directory rows may contain private workspace data; deliberately not Debug.
pub struct DirectoryResponse {
    pub(super) raw: Value,
    pub(super) keys: Option<Vec<String>>,
}
impl DirectoryResponse {
    pub fn into_parts(self) -> (Value, Option<Vec<String>>) {
        (self.raw, self.keys)
    }
}
pub(super) async fn read_body(
    response: wreq::Response,
    status: wreq::StatusCode,
) -> Result<DirectoryResponse, Error> {
    read_body_field(response, status, "results").await
}

pub(super) async fn read_body_field(
    response: wreq::Response,
    status: wreq::StatusCode,
    field: &str,
) -> Result<DirectoryResponse, Error> {
    let bytes = response
        .bytes()
        .await
        .map_err(|e| Error::Transport(format!("decode ({status}): {e}")))?;
    let decoded = if field == "results" {
        decode::decode(&bytes)
    } else {
        decode::decode_field(&bytes, field)
    };
    decoded.map_err(|e| Error::Transport(format!("decode ({status}): {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_values_and_original_key_order_are_one_observation() {
        let response = decode::decode(
            br#"{"ok":true,"results":{"z":{"id":"old"},"10":[],"2":[],"a":[],"z":{"id":"new"}}}"#,
        )
        .unwrap();
        assert_eq!(response.keys.unwrap(), ["2", "10", "z", "a"]);
        assert_eq!(response.raw["results"]["z"]["id"], "new");
        assert!(
            decode::decode(br#"{"ok":true,"results":[]}"#)
                .unwrap()
                .keys
                .is_none()
        );
    }
    #[test]
    fn selected_priority_keys_do_not_replace_results_keys() {
        let bytes=br#"{"ok":true,"results":{"r2":[],"r1":[]},"channel_priority_90d":{"Cz":4,"Ca":4,"Cz":9}}"#;
        let results = decode::decode(bytes).unwrap();
        let priority = decode::decode_field(bytes, "channel_priority_90d").unwrap();
        assert_eq!(results.keys.unwrap(), ["r2", "r1"]);
        assert_eq!(priority.keys.unwrap(), ["Cz", "Ca"]);
        assert_eq!(priority.raw["channel_priority_90d"]["Cz"], 9);
        assert_eq!(priority.raw, results.raw);
    }
    #[test]
    fn priority_array_index_order_and_last_top_level_property_match_json_parse() {
        let response=decode::decode_field(br#"{"channel_priority_90d":{"discard":0},"channel_priority_90d":{"z":1,"10":1,"2":1,"01":1,"4294967295":1,"a":1,"z":2}}"#,"channel_priority_90d").unwrap();
        assert_eq!(
            response.keys.unwrap(),
            ["2", "10", "z", "01", "4294967295", "a"]
        );
        assert!(
            response.raw["channel_priority_90d"]
                .get("discard")
                .is_none()
        );
        assert_eq!(response.raw["channel_priority_90d"]["z"], 2);
    }
    #[test]
    fn absent_empty_and_malformed_priority_shapes_remain_raw_observations() {
        for bytes in [
            br#"{}"#.as_slice(),
            br#"{"channel_priority_90d":null}"#,
            br#"{"channel_priority_90d":false}"#,
            br#"{"channel_priority_90d":[]}"#,
        ] {
            let response = decode::decode_field(bytes, "channel_priority_90d").unwrap();
            assert!(response.keys.is_none());
            assert_eq!(
                response.raw,
                serde_json::from_slice::<Value>(bytes).unwrap()
            );
        }
        let empty = decode::decode_field(br#"{"channel_priority_90d":{}}"#, "channel_priority_90d")
            .unwrap();
        assert_eq!(empty.keys, Some(vec![]));
        assert!(decode::decode_field(br#"{} {}"#, "channel_priority_90d").is_err());
    }
}
