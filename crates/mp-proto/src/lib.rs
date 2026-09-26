//! Shared-file/GNS message protocol. See `README.md` for the design.

use json::JsonValue;

#[derive(Debug, Clone, PartialEq)]
pub enum RelayMessage {
    /// Per-frame local player state, sent unreliable (see README.md on
    /// why `payload` excludes `player_id`).
    PlayerState { player_id: String, payload: JsonValue },
    /// Asks the peer to send a full `Snapshot`. Sent reliable (README.md).
    SnapshotRequest,
    /// A full world snapshot, sent as a single GNS reliable message.
    Snapshot { payload: JsonValue },
}

impl RelayMessage {
    /// Whether this message should be sent as a GNS reliable vs. unreliable
    /// message. Player-state updates are frequent and superseded by the
    /// next one; everything else is control-plane and must arrive.
    pub fn is_reliable(&self) -> bool {
        !matches!(self, RelayMessage::PlayerState { .. })
    }

    pub fn to_json(&self) -> JsonValue {
        match self {
            RelayMessage::PlayerState { player_id, payload } => {
                let mut entries = vec![("player_id".to_string(), JsonValue::String(player_id.clone()))];
                if let JsonValue::Object(fields) = payload {
                    entries.extend(fields.iter().cloned());
                }
                JsonValue::Object(entries)
            }
            RelayMessage::SnapshotRequest => JsonValue::Object(vec![(
                "mp_msg".to_string(),
                JsonValue::String("snap_req".to_string()),
            )]),
            RelayMessage::Snapshot { payload } => JsonValue::Object(vec![
                ("mp_msg".to_string(), JsonValue::String("snapshot".to_string())),
                ("payload".to_string(), payload.clone()),
            ]),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        json::write(&self.to_json()).into_bytes()
    }

    pub fn parse(bytes: &[u8]) -> Option<RelayMessage> {
        let text = std::str::from_utf8(bytes).ok()?;
        let value = json::parse(text).ok()?;
        Self::from_json(&value)
    }

    pub fn from_json(value: &JsonValue) -> Option<RelayMessage> {
        if let Some(mp_msg) = value.get("mp_msg").and_then(JsonValue::as_str) {
            return match mp_msg {
                "snap_req" => Some(RelayMessage::SnapshotRequest),
                "snapshot" => Some(RelayMessage::Snapshot {
                    payload: value.get("payload")?.clone(),
                }),
                _ => None,
            };
        }
        if let Some(player_id) = value.get("player_id").and_then(JsonValue::as_str) {
            let player_id = player_id.to_string();
            // `payload` holds the fields *besides* player_id, so to_json's
            // re-insertion of player_id below can't produce a duplicate.
            let payload = match value {
                JsonValue::Object(fields) => {
                    JsonValue::Object(fields.iter().filter(|(k, _)| k != "player_id").cloned().collect())
                }
                _ => JsonValue::Object(Vec::new()),
            };
            return Some(RelayMessage::PlayerState { player_id, payload });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_state_round_trips_and_is_unreliable() {
        let msg = RelayMessage::PlayerState {
            player_id: "p1".to_string(),
            // Excludes player_id: it lives in the field above, and to_json
            // re-inserts it — payload holding it too would duplicate it.
            payload: json::parse(r#"{"x":1.5,"y":2.0}"#).unwrap(),
        };
        assert!(!msg.is_reliable());
        let bytes = msg.to_bytes();
        let parsed = RelayMessage::parse(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn snapshot_request_round_trips_and_is_reliable() {
        let msg = RelayMessage::SnapshotRequest;
        assert!(msg.is_reliable());
        let bytes = msg.to_bytes();
        assert_eq!(RelayMessage::parse(&bytes).unwrap(), msg);
    }

    #[test]
    fn snapshot_round_trips_and_is_reliable() {
        let msg = RelayMessage::Snapshot {
            payload: json::parse(r#"{"tiles":[1,2,3],"version":7}"#).unwrap(),
        };
        assert!(msg.is_reliable());
        let bytes = msg.to_bytes();
        assert_eq!(RelayMessage::parse(&bytes).unwrap(), msg);
    }

    #[test]
    fn parse_rejects_unrecognized_shapes() {
        assert_eq!(RelayMessage::parse(b"{}"), None);
        assert_eq!(RelayMessage::parse(b"{\"mp_msg\":\"unknown\"}"), None);
        assert_eq!(RelayMessage::parse(b"not json"), None);
    }
}
