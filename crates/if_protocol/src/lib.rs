// if_protocol: Wire protocol shared by client and server.
//
// Keep this crate tiny and dependency-free beyond serde/bincode/if_common.
// Every message that crosses the wire is declared here so the client and
// server agree on the byte layout. Serialized with `bincode` to keep frames
// compact — we do NOT use a self-describing format on purpose.

use if_common::{GridPosition, item::ItemType};
use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// Building type wire code
// -----------------------------------------------------------------------------
//
// We purposefully don't depend on `if_factory` here (server core shouldn't be
// bound to factory internals either). The wire code is a stable u8 mapping
// that the client and server translate to/from their local `BuildingType`.
// Add new variants by appending — never reuse a number.
pub mod building_code {
    pub const MINING_DRILL: u8 = 0;
    pub const TRANSPORT_LINE: u8 = 1;
    pub const SMELTER: u8 = 2;
    pub const ASSEMBLER: u8 = 3;
    pub const GENERATOR: u8 = 4;
}

/// Current protocol version. Bump this any time the wire layout changes in a
/// non-backwards-compatible way so the server can reject stale clients.
pub const PROTOCOL_VERSION: u32 = 1;

/// Default TCP port the server listens on.
pub const DEFAULT_SERVER_PORT: u16 = 7777;

// -----------------------------------------------------------------------------
// Client -> Server
// -----------------------------------------------------------------------------

/// Messages a client can send to the server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Initial handshake with player name.
    Hello { player_name: String },
    /// Request to place a building at a position.
    PlaceBuilding {
        pos: GridPosition,
        building_type: u8,
    },
    /// Request to remove a building.
    RemoveBuilding { pos: GridPosition },
    /// Chat message.
    Chat { text: String },
    /// Ping for RTT measurement.
    Ping { timestamp_ms: u64 },
}

// -----------------------------------------------------------------------------
// Server -> Client
// -----------------------------------------------------------------------------

/// Messages the server can send to a client.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Welcome with assigned player id and world snapshot.
    Welcome {
        player_id: u64,
        snapshot: WorldSnapshot,
    },
    /// Incremental state update.
    StateUpdate {
        frame: u64,
        updates: Vec<EntityUpdate>,
    },
    /// Chat broadcast.
    Chat { from: String, text: String },
    /// Pong response (echoes the client's timestamp).
    Pong { timestamp_ms: u64 },
    /// Error message. The server MAY close the connection after sending this.
    Error { reason: String },
}

/// Full world state sent to a client on connect. Kept intentionally small
/// while the server simulation is still a stub.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorldSnapshot {
    pub tick: u64,
    pub buildings: Vec<NetBuilding>,
    pub resources: Vec<NetResource>,
}

/// A building as it travels over the wire. `id` is a stable server-assigned
/// identifier so the client can correlate updates without knowing the server's
/// Bevy `Entity` ids.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetBuilding {
    pub id: u32,
    pub pos: GridPosition,
    pub building_type: u8,
    pub inventory: Vec<(ItemType, u32)>,
}

/// A resource deposit on the map.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetResource {
    pub pos: GridPosition,
    pub resource: ItemType,
    pub remaining: u32,
}

/// Incremental update applied on top of a `WorldSnapshot`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EntityUpdate {
    BuildingPlaced(NetBuilding),
    BuildingRemoved {
        id: u32,
    },
    InventoryChanged {
        id: u32,
        inventory: Vec<(ItemType, u32)>,
    },
    ResourceRemaining {
        pos: GridPosition,
        remaining: u32,
    },
}

// -----------------------------------------------------------------------------
// Serialization helpers
// -----------------------------------------------------------------------------

/// Encode a `ClientMessage` using bincode's default configuration.
pub fn encode_client(msg: &ClientMessage) -> Result<Vec<u8>, bincode::Error> {
    bincode::serialize(msg)
}

/// Decode a `ClientMessage` using bincode's default configuration.
pub fn decode_client(bytes: &[u8]) -> Result<ClientMessage, bincode::Error> {
    bincode::deserialize(bytes)
}

/// Encode a `ServerMessage` using bincode's default configuration.
pub fn encode_server(msg: &ServerMessage) -> Result<Vec<u8>, bincode::Error> {
    bincode::serialize(msg)
}

/// Decode a `ServerMessage` using bincode's default configuration.
pub fn decode_server(bytes: &[u8]) -> Result<ServerMessage, bincode::Error> {
    bincode::deserialize(bytes)
}

// -----------------------------------------------------------------------------
// Length-prefixed framing
// -----------------------------------------------------------------------------
//
// TCP is a byte stream, not a message stream. We prefix every payload with a
// big-endian u32 length. Small, boring, reliable. Upgrade later to QUIC if we
// need unreliable channels or richer flow control.

/// Maximum allowed frame payload size (in bytes). Any frame that claims to be
/// larger than this is rejected to avoid pathological allocations from
/// malicious or corrupt input.
pub const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024; // 16 MiB

/// Errors produced while encoding or decoding wire frames.
#[derive(Debug)]
pub enum FrameError {
    /// A serialization error from bincode.
    Encoding(bincode::Error),
    /// The declared frame size exceeds `MAX_FRAME_SIZE`.
    TooLarge(u32),
    /// Not enough bytes available to parse a full frame.
    Incomplete,
    /// A length prefix of zero, which is always invalid.
    EmptyFrame,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Encoding(e) => write!(f, "encoding error: {e}"),
            FrameError::TooLarge(n) => write!(f, "frame too large: {n} bytes"),
            FrameError::Incomplete => write!(f, "incomplete frame"),
            FrameError::EmptyFrame => write!(f, "empty frame"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<bincode::Error> for FrameError {
    fn from(e: bincode::Error) -> Self {
        FrameError::Encoding(e)
    }
}

/// Wrap `payload` with a big-endian u32 length prefix and return the full frame.
pub fn frame(payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    let len = payload.len();
    if len as u64 > MAX_FRAME_SIZE as u64 {
        return Err(FrameError::TooLarge(len as u32));
    }
    let mut out = Vec::with_capacity(4 + len);
    out.extend_from_slice(&(len as u32).to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Encode a `ClientMessage` and wrap it in a length-prefixed frame.
pub fn encode_client_frame(msg: &ClientMessage) -> Result<Vec<u8>, FrameError> {
    let bytes = encode_client(msg)?;
    frame(&bytes)
}

/// Encode a `ServerMessage` and wrap it in a length-prefixed frame.
pub fn encode_server_frame(msg: &ServerMessage) -> Result<Vec<u8>, FrameError> {
    let bytes = encode_server(msg)?;
    frame(&bytes)
}

/// Attempt to read one full frame out of `buf`. On success, returns the frame
/// payload bytes and the number of bytes consumed from `buf`. On
/// `FrameError::Incomplete` the caller should read more bytes and retry.
pub fn try_read_frame(buf: &[u8]) -> Result<(Vec<u8>, usize), FrameError> {
    if buf.len() < 4 {
        return Err(FrameError::Incomplete);
    }
    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&buf[..4]);
    let len = u32::from_be_bytes(len_bytes);
    if len == 0 {
        return Err(FrameError::EmptyFrame);
    }
    if len > MAX_FRAME_SIZE {
        return Err(FrameError::TooLarge(len));
    }
    let total = 4 + len as usize;
    if buf.len() < total {
        return Err(FrameError::Incomplete);
    }
    let payload = buf[4..total].to_vec();
    Ok((payload, total))
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot() -> WorldSnapshot {
        WorldSnapshot {
            tick: 42,
            buildings: vec![NetBuilding {
                id: 1,
                pos: GridPosition::new(3, 4),
                building_type: building_code::MINING_DRILL,
                inventory: vec![(ItemType::IronOre, 5), (ItemType::CopperOre, 2)],
            }],
            resources: vec![NetResource {
                pos: GridPosition::new(10, 11),
                resource: ItemType::IronOre,
                remaining: 1000,
            }],
        }
    }

    #[test]
    fn client_hello_roundtrip() {
        let msg = ClientMessage::Hello {
            player_name: "alice".to_string(),
        };
        let bytes = encode_client(&msg).unwrap();
        let decoded = decode_client(&bytes).unwrap();
        match decoded {
            ClientMessage::Hello { player_name } => assert_eq!(player_name, "alice"),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn client_place_building_roundtrip() {
        let msg = ClientMessage::PlaceBuilding {
            pos: GridPosition::new(2, 5),
            building_type: building_code::SMELTER,
        };
        let bytes = encode_client(&msg).unwrap();
        let decoded = decode_client(&bytes).unwrap();
        match decoded {
            ClientMessage::PlaceBuilding { pos, building_type } => {
                assert_eq!(pos, GridPosition::new(2, 5));
                assert_eq!(building_type, building_code::SMELTER);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn client_remove_building_roundtrip() {
        let msg = ClientMessage::RemoveBuilding {
            pos: GridPosition::new(7, 8),
        };
        let bytes = encode_client(&msg).unwrap();
        let decoded = decode_client(&bytes).unwrap();
        match decoded {
            ClientMessage::RemoveBuilding { pos } => assert_eq!(pos, GridPosition::new(7, 8)),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn client_chat_roundtrip() {
        let msg = ClientMessage::Chat {
            text: "hello, world!".to_string(),
        };
        let bytes = encode_client(&msg).unwrap();
        let decoded = decode_client(&bytes).unwrap();
        match decoded {
            ClientMessage::Chat { text } => assert_eq!(text, "hello, world!"),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn client_ping_roundtrip() {
        let msg = ClientMessage::Ping {
            timestamp_ms: 12345,
        };
        let bytes = encode_client(&msg).unwrap();
        let decoded = decode_client(&bytes).unwrap();
        match decoded {
            ClientMessage::Ping { timestamp_ms } => assert_eq!(timestamp_ms, 12345),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn server_welcome_roundtrip() {
        let msg = ServerMessage::Welcome {
            player_id: 9,
            snapshot: sample_snapshot(),
        };
        let bytes = encode_server(&msg).unwrap();
        let decoded = decode_server(&bytes).unwrap();
        match decoded {
            ServerMessage::Welcome {
                player_id,
                snapshot,
            } => {
                assert_eq!(player_id, 9);
                assert_eq!(snapshot.tick, 42);
                assert_eq!(snapshot.buildings.len(), 1);
                assert_eq!(snapshot.resources.len(), 1);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn server_state_update_roundtrip() {
        let msg = ServerMessage::StateUpdate {
            frame: 7,
            updates: vec![
                EntityUpdate::BuildingPlaced(NetBuilding {
                    id: 2,
                    pos: GridPosition::new(1, 1),
                    building_type: building_code::ASSEMBLER,
                    inventory: vec![],
                }),
                EntityUpdate::BuildingRemoved { id: 2 },
                EntityUpdate::InventoryChanged {
                    id: 3,
                    inventory: vec![(ItemType::HullPlate, 12)],
                },
                EntityUpdate::ResourceRemaining {
                    pos: GridPosition::new(4, 4),
                    remaining: 900,
                },
            ],
        };
        let bytes = encode_server(&msg).unwrap();
        let decoded = decode_server(&bytes).unwrap();
        match decoded {
            ServerMessage::StateUpdate { frame, updates } => {
                assert_eq!(frame, 7);
                assert_eq!(updates.len(), 4);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn server_chat_roundtrip() {
        let msg = ServerMessage::Chat {
            from: "alice".to_string(),
            text: "gg".to_string(),
        };
        let bytes = encode_server(&msg).unwrap();
        let decoded = decode_server(&bytes).unwrap();
        match decoded {
            ServerMessage::Chat { from, text } => {
                assert_eq!(from, "alice");
                assert_eq!(text, "gg");
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn server_pong_roundtrip() {
        let msg = ServerMessage::Pong {
            timestamp_ms: 99999,
        };
        let bytes = encode_server(&msg).unwrap();
        let decoded = decode_server(&bytes).unwrap();
        match decoded {
            ServerMessage::Pong { timestamp_ms } => assert_eq!(timestamp_ms, 99999),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn server_error_roundtrip() {
        let msg = ServerMessage::Error {
            reason: "protocol mismatch".to_string(),
        };
        let bytes = encode_server(&msg).unwrap();
        let decoded = decode_server(&bytes).unwrap();
        match decoded {
            ServerMessage::Error { reason } => assert_eq!(reason, "protocol mismatch"),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn frame_roundtrip_single() {
        let msg = ClientMessage::Ping { timestamp_ms: 1 };
        let framed = encode_client_frame(&msg).unwrap();
        let (payload, consumed) = try_read_frame(&framed).unwrap();
        assert_eq!(consumed, framed.len());
        let decoded = decode_client(&payload).unwrap();
        assert!(matches!(decoded, ClientMessage::Ping { timestamp_ms: 1 }));
    }

    #[test]
    fn frame_roundtrip_multiple_concatenated() {
        let a = encode_client_frame(&ClientMessage::Ping { timestamp_ms: 1 }).unwrap();
        let b = encode_client_frame(&ClientMessage::Ping { timestamp_ms: 2 }).unwrap();
        let mut stream = Vec::new();
        stream.extend_from_slice(&a);
        stream.extend_from_slice(&b);

        let (p1, n1) = try_read_frame(&stream).unwrap();
        assert_eq!(n1, a.len());
        let msg1 = decode_client(&p1).unwrap();
        assert!(matches!(msg1, ClientMessage::Ping { timestamp_ms: 1 }));

        let (p2, n2) = try_read_frame(&stream[n1..]).unwrap();
        assert_eq!(n2, b.len());
        let msg2 = decode_client(&p2).unwrap();
        assert!(matches!(msg2, ClientMessage::Ping { timestamp_ms: 2 }));
    }

    #[test]
    fn frame_incomplete_header() {
        let err = try_read_frame(&[0, 0, 0]).unwrap_err();
        assert!(matches!(err, FrameError::Incomplete));
    }

    #[test]
    fn frame_incomplete_payload() {
        // Length prefix says 10, but only 2 payload bytes are available.
        let mut buf = Vec::new();
        buf.extend_from_slice(&10u32.to_be_bytes());
        buf.extend_from_slice(&[1, 2]);
        let err = try_read_frame(&buf).unwrap_err();
        assert!(matches!(err, FrameError::Incomplete));
    }

    #[test]
    fn frame_rejects_empty() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_be_bytes());
        let err = try_read_frame(&buf).unwrap_err();
        assert!(matches!(err, FrameError::EmptyFrame));
    }

    #[test]
    fn frame_rejects_too_large() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(MAX_FRAME_SIZE + 1).to_be_bytes());
        let err = try_read_frame(&buf).unwrap_err();
        assert!(matches!(err, FrameError::TooLarge(_)));
    }

    #[test]
    fn server_frame_roundtrip() {
        let msg = ServerMessage::Chat {
            from: "server".to_string(),
            text: "motd".to_string(),
        };
        let framed = encode_server_frame(&msg).unwrap();
        let (payload, _) = try_read_frame(&framed).unwrap();
        let decoded = decode_server(&payload).unwrap();
        match decoded {
            ServerMessage::Chat { from, text } => {
                assert_eq!(from, "server");
                assert_eq!(text, "motd");
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }
}
