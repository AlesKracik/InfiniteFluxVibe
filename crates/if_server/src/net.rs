// net.rs: tokio-backed TCP networking for if_server.
//
// Design:
// - One dedicated OS thread owns a tokio `current_thread` runtime.
// - Bevy talks to tokio via `std::sync::mpsc` channels (blocking types are
//   fine because Bevy only uses `try_send`/`try_recv`, never blocking).
// - Each client connection spawns a tokio task that owns its half of the
//   split TCP stream; a per-client `tokio::sync::mpsc` channel carries the
//   outbound writes back.
//
// This keeps the Bevy scheduler completely free of async runtimes while still
// letting us enjoy tokio's ergonomics on the network side.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use bevy::prelude::Resource;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc::{UnboundedReceiver as TokioReceiver, UnboundedSender as TokioSender};

use if_protocol::{
    ClientMessage, FrameError, MAX_FRAME_SIZE, ServerMessage, decode_client, encode_server_frame,
};

// -----------------------------------------------------------------------------
// Bridge types
// -----------------------------------------------------------------------------

/// Events flowing from the network thread into Bevy.
#[derive(Debug)]
pub enum NetInbound {
    Connected { client_id: u64 },
    Disconnected { client_id: u64 },
    Message { client_id: u64, msg: ClientMessage },
}

/// Commands flowing from Bevy into the network thread.
#[derive(Debug)]
pub enum NetOutbound {
    ToClient { client_id: u64, msg: ServerMessage },
    Broadcast { msg: ServerMessage },
}

/// Handle held by the Bevy side. Stored as a Bevy `Resource`.
///
/// `std::sync::mpsc::Receiver` is `Send` but `!Sync`, so we wrap it in a
/// `Mutex` to satisfy Bevy's `Resource` bound. The Bevy side only ever touches
/// it from a single system (`drain_network_inbound`), so contention is nil.
#[derive(Resource)]
pub struct NetHandle {
    pub bind_addr: SocketAddr,
    pub inbound: Mutex<std::sync::mpsc::Receiver<NetInbound>>,
    pub outbound: std::sync::mpsc::Sender<NetOutbound>,
}

// -----------------------------------------------------------------------------
// Thread + runtime spawn
// -----------------------------------------------------------------------------

/// Spawn the dedicated networking thread and return a handle for Bevy to use.
pub fn spawn_network_thread(bind_addr: SocketAddr) -> NetHandle {
    let (inbound_tx, inbound_rx) = std::sync::mpsc::channel::<NetInbound>();
    let (outbound_tx, outbound_rx) = std::sync::mpsc::channel::<NetOutbound>();

    thread::Builder::new()
        .name("if_server-net".to_string())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()
                .expect("building tokio runtime");
            rt.block_on(async move {
                if let Err(e) = run_network(bind_addr, inbound_tx, outbound_rx).await {
                    eprintln!("if_server network error: {e}");
                }
            });
        })
        .expect("spawning net thread");

    NetHandle {
        bind_addr,
        inbound: Mutex::new(inbound_rx),
        outbound: outbound_tx,
    }
}

// -----------------------------------------------------------------------------
// Main network task
// -----------------------------------------------------------------------------

/// Shared map of connected clients. `ClientEntry` holds the per-client write
/// channel — the accept loop inserts entries and the broadcaster reads them.
struct ClientEntry {
    writer: TokioSender<Vec<u8>>,
}

type ClientMap = Arc<Mutex<std::collections::HashMap<u64, ClientEntry>>>;

async fn run_network(
    bind_addr: SocketAddr,
    inbound_tx: std::sync::mpsc::Sender<NetInbound>,
    outbound_rx: std::sync::mpsc::Receiver<NetOutbound>,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(bind_addr).await?;
    let clients: ClientMap = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let next_id = Arc::new(AtomicU64::new(1));

    // Spawn the outbound dispatcher. It reads commands from the std mpsc
    // channel (polled in a blocking-friendly loop via `spawn_blocking`) and
    // forwards them to the right per-client channels.
    let clients_out = clients.clone();
    tokio::spawn(async move {
        // We wrap the std Receiver in Arc<Mutex<...>> because it's `!Sync`.
        let outbound_rx = Arc::new(Mutex::new(outbound_rx));
        loop {
            let rx = outbound_rx.clone();
            // Block on a dedicated blocking worker so we never starve tokio.
            let res = tokio::task::spawn_blocking(move || {
                let guard = rx.lock().expect("outbound rx poisoned");
                guard.recv()
            })
            .await;
            match res {
                Ok(Ok(cmd)) => dispatch_outbound(&clients_out, cmd).await,
                // Channel closed: Bevy shut down, so we exit quietly.
                Ok(Err(_)) => break,
                Err(e) => {
                    eprintln!("outbound blocking task panicked: {e}");
                    break;
                }
            }
        }
    });

    // Accept loop.
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("accept error: {e}");
                continue;
            }
        };
        if let Err(e) = stream.set_nodelay(true) {
            eprintln!("set_nodelay failed: {e}");
        }

        let client_id = next_id.fetch_add(1, Ordering::Relaxed);
        let (read_half, write_half) = stream.into_split();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        {
            let mut map = clients.lock().expect("client map poisoned");
            map.insert(client_id, ClientEntry { writer: tx });
        }

        if inbound_tx
            .send(NetInbound::Connected { client_id })
            .is_err()
        {
            // Bevy is gone; drop the connection.
            return Ok(());
        }

        // Writer task: framed ServerMessage bytes -> socket.
        tokio::spawn(writer_task(write_half, rx));

        // Reader task: socket -> framed ClientMessage -> inbound channel.
        let inbound_tx_clone = inbound_tx.clone();
        let clients_clone = clients.clone();
        tokio::spawn(async move {
            let res = reader_task(read_half, client_id, inbound_tx_clone.clone()).await;
            if let Err(e) = res {
                eprintln!("client {client_id} ({peer}) reader error: {e}");
            }
            // Remove the client from the shared map so the writer task exits
            // once its channel is dropped.
            {
                let mut map = clients_clone.lock().expect("client map poisoned");
                map.remove(&client_id);
            }
            let _ = inbound_tx_clone.send(NetInbound::Disconnected { client_id });
        });
    }
}

async fn dispatch_outbound(clients: &ClientMap, cmd: NetOutbound) {
    match cmd {
        NetOutbound::ToClient { client_id, msg } => {
            let bytes = match encode_server_frame(&msg) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("encode error: {e}");
                    return;
                }
            };
            let map = clients.lock().expect("client map poisoned");
            if let Some(entry) = map.get(&client_id) {
                let _ = entry.writer.send(bytes);
            }
        }
        NetOutbound::Broadcast { msg } => {
            let bytes = match encode_server_frame(&msg) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("encode error: {e}");
                    return;
                }
            };
            let map = clients.lock().expect("client map poisoned");
            for entry in map.values() {
                let _ = entry.writer.send(bytes.clone());
            }
        }
    }
}

async fn writer_task(mut write_half: OwnedWriteHalf, mut rx: TokioReceiver<Vec<u8>>) {
    while let Some(frame) = rx.recv().await {
        if let Err(e) = write_half.write_all(&frame).await {
            eprintln!("write error: {e}");
            break;
        }
    }
    let _ = write_half.shutdown().await;
}

async fn reader_task(
    mut read_half: OwnedReadHalf,
    client_id: u64,
    inbound_tx: std::sync::mpsc::Sender<NetInbound>,
) -> std::io::Result<()> {
    // Streaming frame decoder: accumulate bytes, extract zero or more
    // complete frames each loop iteration.
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    loop {
        let n = read_half.read(&mut tmp).await?;
        if n == 0 {
            // Clean EOF.
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);

        loop {
            match if_protocol::try_read_frame(&buf) {
                Ok((payload, consumed)) => {
                    // Drop consumed bytes from the front of the buffer.
                    buf.drain(..consumed);
                    match decode_client(&payload) {
                        Ok(msg) => {
                            if inbound_tx
                                .send(NetInbound::Message { client_id, msg })
                                .is_err()
                            {
                                // Bevy is gone.
                                return Ok(());
                            }
                        }
                        Err(e) => {
                            eprintln!("client {client_id} decode error: {e}");
                            return Ok(());
                        }
                    }
                }
                Err(FrameError::Incomplete) => break,
                Err(FrameError::TooLarge(n)) => {
                    eprintln!(
                        "client {client_id} sent oversized frame ({n} bytes, max {MAX_FRAME_SIZE}), dropping"
                    );
                    return Ok(());
                }
                Err(FrameError::EmptyFrame) => {
                    eprintln!("client {client_id} sent empty frame, dropping");
                    return Ok(());
                }
                Err(FrameError::Encoding(e)) => {
                    eprintln!("client {client_id} frame encoding error: {e}");
                    return Ok(());
                }
            }
        }
    }
}
