//! Pure payload decisions shared by the RabbitMQ delivery consumers.

use project_balloon_contracts::WorkerHeartbeat;
use uuid::Uuid;

/// Producers stamp the AMQP `message_id` property. A present property must
/// match the payload's own id; an absent property stays accepted.
pub(super) fn message_id_mismatch(property_id: Option<&str>, payload_message_id: Uuid) -> bool {
    property_id.is_some_and(|property| property != payload_message_id.to_string())
}

/// Outcome of parsing a heartbeat delivery before any database work happens.
pub(super) enum HeartbeatPayload {
    /// Well-formed and passes `WorkerHeartbeat::validate`.
    Accepted(WorkerHeartbeat),
    /// Parses but violates the heartbeat contract; the payload stays available
    /// for logging its worker id.
    Invalid(WorkerHeartbeat),
    /// Not a JSON heartbeat at all.
    Malformed(serde_json::Error),
}

pub(super) fn parse_heartbeat(data: &[u8]) -> HeartbeatPayload {
    match serde_json::from_slice::<WorkerHeartbeat>(data) {
        Ok(value) if value.validate().is_ok() => HeartbeatPayload::Accepted(value),
        Ok(value) => HeartbeatPayload::Invalid(value),
        Err(error) => HeartbeatPayload::Malformed(error),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use project_balloon_contracts::{WORKER_HEARTBEAT_SCHEMA_VERSION, WorkerHeartbeat};
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    use super::{HeartbeatPayload, message_id_mismatch, parse_heartbeat};

    fn heartbeat() -> WorkerHeartbeat {
        let now = OffsetDateTime::now_utc();
        WorkerHeartbeat {
            schema_version: WORKER_HEARTBEAT_SCHEMA_VERSION,
            message_id: Uuid::new_v4(),
            worker_id: "payload-test-worker".to_owned(),
            instance_id: Uuid::new_v4(),
            started_at: now - Duration::MINUTE,
            occurred_at: now,
            capacity: 2,
            active_tasks: 0,
            languages: vec!["cpp".to_owned()],
            runtime_versions: BTreeMap::new(),
            sandbox_runtime: None,
        }
    }

    #[test]
    fn message_id_gate_rejects_only_a_present_and_mismatched_property() {
        let message_id = Uuid::new_v4();
        assert!(!message_id_mismatch(None, message_id), "absent property must stay accepted");
        assert!(
            !message_id_mismatch(Some(message_id.to_string().as_str()), message_id),
            "matching property must stay accepted"
        );
        assert!(
            message_id_mismatch(Some(Uuid::new_v4().to_string().as_str()), message_id),
            "mismatched property must be rejected"
        );
    }

    #[test]
    fn heartbeat_payloads_split_into_accepted_invalid_and_malformed() {
        let valid = heartbeat();
        let encoded = serde_json::to_vec(&valid).expect("serialize heartbeat");
        let HeartbeatPayload::Accepted(parsed) = parse_heartbeat(&encoded) else {
            panic!("valid heartbeat payload must be accepted");
        };
        assert_eq!(parsed.worker_id, valid.worker_id);

        let mut invalid = heartbeat();
        invalid.capacity = 0;
        let encoded = serde_json::to_vec(&invalid).expect("serialize heartbeat");
        let HeartbeatPayload::Invalid(parsed) = parse_heartbeat(&encoded) else {
            panic!("contract-violating heartbeat payload must be invalid");
        };
        assert_eq!(
            parsed.worker_id, invalid.worker_id,
            "invalid payloads stay available for logging"
        );

        let HeartbeatPayload::Malformed(_) = parse_heartbeat(b"{not-json") else {
            panic!("non-JSON heartbeat payload must be malformed");
        };
    }
}
