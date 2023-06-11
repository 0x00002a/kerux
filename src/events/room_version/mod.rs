use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::util::{domain::Domain, mxid::RoomId, MatrixId};

use super::{room_version::v4::PduV4, Event, EventContent};

pub mod v4;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum VersionedPdu {
    V4(PduV4),
}

/// Getter functions for all non-version-specific fields
impl VersionedPdu {
    pub fn event_content(&self) -> &EventContent {
        match self {
            VersionedPdu::V4(pdu) => &pdu.event_content,
        }
    }

    pub fn room_id(&self) -> &RoomId {
        match self {
            VersionedPdu::V4(pdu) => &pdu.room_id,
        }
    }

    pub fn sender(&self) -> &MatrixId {
        match self {
            VersionedPdu::V4(pdu) => &pdu.sender,
        }
    }

    pub fn state_key(&self) -> Option<&str> {
        match self {
            VersionedPdu::V4(pdu) => pdu.state_key.as_deref(),
        }
    }

    pub fn unsigned(&self) -> Option<&JsonValue> {
        match self {
            VersionedPdu::V4(pdu) => pdu.unsigned.as_ref(),
        }
    }

    pub fn redacts(&self) -> Option<&str> {
        match self {
            VersionedPdu::V4(pdu) => pdu.redacts.as_deref(),
        }
    }

    pub fn origin(&self) -> &Domain {
        match self {
            VersionedPdu::V4(pdu) => &pdu.origin,
        }
    }

    pub fn origin_server_ts(&self) -> i64 {
        match self {
            VersionedPdu::V4(pdu) => pdu.origin_server_ts,
        }
    }

    pub fn prev_events(&self) -> &[String] {
        match self {
            VersionedPdu::V4(pdu) => &pdu.prev_events,
        }
    }

    pub fn auth_events(&self) -> &[String] {
        match self {
            VersionedPdu::V4(pdu) => &pdu.auth_events,
        }
    }

    pub(super) fn depth(&self) -> i64 {
        match self {
            VersionedPdu::V4(pdu) => pdu.depth,
        }
    }

    pub fn redact(self) -> Self {
        match self {
            VersionedPdu::V4(pdu) => VersionedPdu::V4(pdu.redact()),
        }
    }

    // TODO: actually completely wrong
    // event_id should probably be stored in StoredPdu because it is not part of a pdu
    pub fn event_id(&self) -> String {
        match self {
            VersionedPdu::V4(pdu) => pdu.event_id(),
        }
    }
}

/// Delegations to version-specific functionality
impl VersionedPdu {
    pub fn into_client_format(self) -> Event {
        match self {
            VersionedPdu::V4(pdu) => pdu.into_client_format(),
        }
    }
}
