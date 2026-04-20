// net.rs: optional client-side networking for Infinite Flux.
//
// Design goals:
// - The single-player experience must keep working with zero changes if the
//   player never presses the "connect" key. Everything here is behind a
//   resource that only becomes Active when the user actually connects.
// - No tokio on the client. We use `std::net::TcpStream` in non-blocking mode
//   and poll it from a Bevy system. This avoids pulling a whole async runtime
//   into the rendering process.
//
// Keybinds added:
// - F9  : toggle connection to 127.0.0.1:7777
// - T   : toggle chat panel
// - Enter (in chat input): send chat message
// - Esc (in chat input): close chat panel

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use if_protocol::{
    ClientMessage, DEFAULT_SERVER_PORT, FrameError, ServerMessage, decode_server,
    encode_client_frame, try_read_frame,
};

// -----------------------------------------------------------------------------
// Plugin
// -----------------------------------------------------------------------------

pub struct ClientNetPlugin;

impl Plugin for ClientNetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NetConnection>()
            .init_resource::<ChatMessages>()
            .init_resource::<ChatUi>()
            .add_systems(
                Update,
                (
                    toggle_connection_hotkey,
                    receive_messages_system,
                    toggle_chat_panel_hotkey,
                    chat_panel_ui,
                ),
            );
    }
}

// -----------------------------------------------------------------------------
// Resources
// -----------------------------------------------------------------------------

/// The network connection to the server. Optional — when `None` the client
/// runs as pure single-player. The stream is always in non-blocking mode.
#[derive(Resource, Default)]
pub struct NetConnection {
    inner: Option<ActiveConnection>,
    /// Partial inbound bytes waiting for more data to form a complete frame.
    read_buf: Vec<u8>,
    /// The player id assigned by the server on Welcome.
    pub player_id: Option<u64>,
}

struct ActiveConnection {
    stream: TcpStream,
    addr: SocketAddr,
}

impl NetConnection {
    pub fn is_connected(&self) -> bool {
        self.inner.is_some()
    }

    pub fn addr(&self) -> Option<SocketAddr> {
        self.inner.as_ref().map(|c| c.addr)
    }

    /// Attempt to open a non-blocking TCP connection to the server.
    pub fn connect(&mut self, addr: SocketAddr) -> std::io::Result<()> {
        // Use connect_timeout for a snappier UX than blocking on SYN.
        let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500))?;
        stream.set_nonblocking(true)?;
        let _ = stream.set_nodelay(true);
        self.inner = Some(ActiveConnection { stream, addr });
        self.read_buf.clear();
        self.player_id = None;
        Ok(())
    }

    pub fn disconnect(&mut self) {
        if let Some(conn) = self.inner.take() {
            let _ = conn.stream.shutdown(std::net::Shutdown::Both);
        }
        self.read_buf.clear();
        self.player_id = None;
    }

    /// Serialize and send `msg`. Returns Err if not connected or on IO error.
    pub fn send(&mut self, msg: &ClientMessage) -> Result<(), NetSendError> {
        let Some(conn) = self.inner.as_mut() else {
            return Err(NetSendError::NotConnected);
        };
        let frame = encode_client_frame(msg).map_err(NetSendError::Frame)?;
        // Best-effort write. For MVP we assume the frame fits in the send
        // buffer; non-blocking write may do a partial write but on loopback
        // with small payloads this is vanishingly rare. We log and drop the
        // connection if we hit WouldBlock mid-write to avoid silent data loss.
        match conn.stream.write_all(&frame) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                warn!("network send would block; dropping connection");
                self.disconnect();
                Err(NetSendError::Io(e))
            }
            Err(e) => {
                warn!("network send failed: {e}");
                self.disconnect();
                Err(NetSendError::Io(e))
            }
        }
    }
}

#[derive(Debug)]
pub enum NetSendError {
    NotConnected,
    Frame(FrameError),
    Io(std::io::Error),
}

impl std::fmt::Display for NetSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetSendError::NotConnected => write!(f, "not connected"),
            NetSendError::Frame(e) => write!(f, "frame error: {e}"),
            NetSendError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for NetSendError {}

/// Ring buffer of recent chat messages for display.
#[derive(Resource, Default)]
pub struct ChatMessages {
    pub entries: Vec<ChatEntry>,
}

#[derive(Clone, Debug)]
pub struct ChatEntry {
    pub from: String,
    pub text: String,
}

impl ChatMessages {
    const MAX: usize = 64;

    pub fn push(&mut self, from: impl Into<String>, text: impl Into<String>) {
        self.entries.push(ChatEntry {
            from: from.into(),
            text: text.into(),
        });
        if self.entries.len() > Self::MAX {
            let drop_n = self.entries.len() - Self::MAX;
            self.entries.drain(..drop_n);
        }
    }
}

/// UI state for the chat panel.
#[derive(Resource, Default)]
pub struct ChatUi {
    pub open: bool,
    pub draft: String,
    /// True once the egui frame is rendered so we can focus the input.
    pub focus_input: bool,
}

// -----------------------------------------------------------------------------
// Systems
// -----------------------------------------------------------------------------

/// F9 toggles the connection. Keep it behind a hotkey so single-player is
/// never disturbed.
pub fn toggle_connection_hotkey(
    keys: Res<ButtonInput<KeyCode>>,
    mut conn: ResMut<NetConnection>,
    mut chat: ResMut<ChatMessages>,
) {
    if !keys.just_pressed(KeyCode::F9) {
        return;
    }
    if conn.is_connected() {
        info!("disconnecting from server");
        conn.disconnect();
        chat.push("system", "disconnected");
        return;
    }
    let addr = SocketAddr::from(([127, 0, 0, 1], DEFAULT_SERVER_PORT));
    match conn.connect(addr) {
        Ok(()) => {
            info!("connected to {addr}");
            chat.push("system", format!("connected to {addr}"));
            // Send a Hello so the server knows who we are. `send` handles
            // the not-yet-ready handshake gracefully.
            let _ = conn.send(&ClientMessage::Hello {
                player_name: "player".to_string(),
            });
        }
        Err(e) => {
            warn!("connect failed: {e}");
            chat.push("system", format!("connect failed: {e}"));
        }
    }
}

/// Poll the non-blocking TCP stream, decode frames, and fan out messages.
pub fn receive_messages_system(mut conn: ResMut<NetConnection>, mut chat: ResMut<ChatMessages>) {
    if !conn.is_connected() {
        return;
    }

    // Read whatever is available right now.
    let mut tmp = [0u8; 4096];
    let mut drained = false;
    loop {
        // Borrow the stream mutably only inside the loop body so we can also
        // borrow `conn.read_buf` separately later.
        let result = {
            let conn_mut = &mut *conn;
            let Some(active) = conn_mut.inner.as_mut() else {
                return;
            };
            active.stream.read(&mut tmp)
        };
        match result {
            Ok(0) => {
                info!("server closed connection");
                conn.disconnect();
                chat.push("system", "server closed connection");
                return;
            }
            Ok(n) => {
                conn.read_buf.extend_from_slice(&tmp[..n]);
                drained = true;
                // Keep reading until WouldBlock.
                continue;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                warn!("read error: {e}");
                conn.disconnect();
                chat.push("system", format!("read error: {e}"));
                return;
            }
        }
    }

    if !drained && conn.read_buf.is_empty() {
        return;
    }

    // Pull complete frames out of `read_buf`. We decode inside the loop and
    // dispatch straight into chat / connection state updates.
    loop {
        match try_read_frame(&conn.read_buf) {
            Ok((payload, consumed)) => {
                conn.read_buf.drain(..consumed);
                match decode_server(&payload) {
                    Ok(msg) => handle_server_message(&mut conn, &mut chat, msg),
                    Err(e) => {
                        warn!("decode error: {e}");
                        conn.disconnect();
                        chat.push("system", "bad message from server; disconnected");
                        return;
                    }
                }
            }
            Err(FrameError::Incomplete) => return,
            Err(e) => {
                warn!("frame error: {e}");
                conn.disconnect();
                chat.push("system", format!("frame error: {e}"));
                return;
            }
        }
    }
}

fn handle_server_message(conn: &mut NetConnection, chat: &mut ChatMessages, msg: ServerMessage) {
    match msg {
        ServerMessage::Welcome { player_id, .. } => {
            conn.player_id = Some(player_id);
            chat.push("system", format!("welcome! you are player {player_id}"));
        }
        ServerMessage::StateUpdate { frame, updates } => {
            // For the foundation we just log updates. A follow-up PR will
            // apply them to the local ECS (building spawn/remove, inventory
            // sync) once the server-side simulation actually produces any.
            debug!(
                "received state update frame={frame} with {} entries",
                updates.len()
            );
        }
        ServerMessage::Chat { from, text } => {
            chat.push(from, text);
        }
        ServerMessage::Pong { timestamp_ms } => {
            debug!("pong {timestamp_ms}");
        }
        ServerMessage::Error { reason } => {
            warn!("server error: {reason}");
            chat.push("server", format!("error: {reason}"));
        }
    }
}

/// Pressing `T` (when not already typing) toggles the chat panel.
pub fn toggle_chat_panel_hotkey(keys: Res<ButtonInput<KeyCode>>, mut chat_ui: ResMut<ChatUi>) {
    // If the panel is open, KeyT is almost certainly the user typing. Only
    // react when the panel is closed — closing happens via Esc inside egui.
    if chat_ui.open {
        return;
    }
    if keys.just_pressed(KeyCode::KeyT) {
        chat_ui.open = true;
        chat_ui.focus_input = true;
    }
}

/// Floating chat window. Rendered unconditionally so the player can see
/// connection status messages even before opening the input.
pub fn chat_panel_ui(
    mut contexts: EguiContexts,
    mut chat_ui: ResMut<ChatUi>,
    chat: Res<ChatMessages>,
    mut conn: ResMut<NetConnection>,
    mut warmup: Local<u8>,
) {
    // Skip early frames — egui's begin_pass may not have run yet
    // when the window is first created.
    if *warmup < 3 {
        *warmup += 1;
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Always show a compact status strip in the bottom-left so the player
    // sees whether they're in multiplayer at a glance.
    egui::Area::new(egui::Id::new("if_client_chat_status"))
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(8.0, -8.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(ui.visuals().panel_fill)
                .show(ui, |ui| {
                    let status = match conn.addr() {
                        Some(a) => format!("online: {a}"),
                        None => "offline (press F9 to connect)".to_string(),
                    };
                    ui.label(status);
                    ui.label(format!(
                        "chat: press T to open ({} messages)",
                        chat.entries.len()
                    ));
                });
        });

    if !chat_ui.open {
        return;
    }

    let mut open = chat_ui.open;
    let mut draft = std::mem::take(&mut chat_ui.draft);
    let mut send_now = false;
    let mut close_now = false;
    let focus_input = chat_ui.focus_input;

    egui::Window::new("Chat")
        .open(&mut open)
        .default_pos(egui::pos2(16.0, 360.0))
        .default_size(egui::vec2(360.0, 260.0))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for entry in chat.entries.iter() {
                        ui.label(format!("{}: {}", entry.from, entry.text));
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut draft)
                        .desired_width(ui.available_width() - 80.0)
                        .hint_text("Say something…"),
                );
                if focus_input {
                    response.request_focus();
                }
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    send_now = true;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close_now = true;
                }
                if ui.button("Send").clicked() {
                    send_now = true;
                }
            });
            ui.label("Enter to send, Esc to close");
        });

    if send_now {
        let text = draft.trim().to_string();
        if !text.is_empty() {
            if conn.is_connected() {
                let _ = conn.send(&ClientMessage::Chat { text: text.clone() });
            } else {
                // Offline: just echo locally so the UI still feels alive.
                // We grab a mutable ChatMessages via a separate system call;
                // for simplicity here we skip the local echo.
            }
            draft.clear();
        }
    }

    if close_now {
        open = false;
    }

    chat_ui.open = open;
    chat_ui.draft = draft;
    chat_ui.focus_input = false;
}
