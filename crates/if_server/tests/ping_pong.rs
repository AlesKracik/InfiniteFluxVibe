// Integration test: bring up the server network thread on a free port,
// connect a tokio TCP client, and verify the welcome + ping/pong round-trip.

use std::net::{SocketAddr, TcpListener as StdTcpListener};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use if_protocol::{
    ClientMessage, FrameError, ServerMessage, decode_server, encode_client_frame, try_read_frame,
};

// Pull in the network module of the binary. The `path` attribute works
// because Cargo builds integration tests with full access to the crate.
// Dead-code warnings fire here because the test only exercises a subset of
// the module; those variants are still used by the binary.
#[allow(dead_code)]
#[path = "../src/net.rs"]
mod net;

fn free_loopback_port() -> u16 {
    // Bind to port 0 to let the OS pick a free port, then release it.
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("bind zero-port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

async fn read_one_server_message(stream: &mut TcpStream) -> ServerMessage {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    loop {
        let n = stream.read(&mut tmp).await.expect("read");
        assert!(n > 0, "server closed connection unexpectedly");
        buf.extend_from_slice(&tmp[..n]);
        match try_read_frame(&buf) {
            Ok((payload, _)) => {
                return decode_server(&payload).expect("decode server message");
            }
            Err(FrameError::Incomplete) => continue,
            Err(e) => panic!("frame error: {e}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn welcome_and_ping_pong() {
    let port = free_loopback_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    // Spawn the network thread. The handle's `inbound`/`outbound` channels
    // would normally be drained by Bevy; here we drive them from the test.
    let handle = net::spawn_network_thread(addr);

    // Retry connect for a short window to avoid races with the listener
    // being ready. A handful of 10ms attempts is plenty on loopback.
    let mut stream = None;
    for _ in 0..50 {
        match TcpStream::connect(addr).await {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
        }
    }
    let mut stream = stream.expect("connect to test server");

    // Simulate what Bevy's drain_network_inbound would do: consume an inbound
    // event and echo a Welcome back. We run this on a blocking task because
    // NetHandle uses std mpsc receivers which block the current thread.
    let outbound = handle.outbound.clone();
    let welcome_task = tokio::task::spawn_blocking(move || {
        // Wait for Connected.
        let event = handle
            .inbound
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("connected event");
        let client_id = match event {
            net::NetInbound::Connected { client_id } => client_id,
            other => panic!("unexpected event: {other:?}"),
        };
        outbound
            .send(net::NetOutbound::ToClient {
                client_id,
                msg: ServerMessage::Welcome {
                    player_id: client_id,
                    snapshot: if_protocol::WorldSnapshot::default(),
                },
            })
            .expect("send welcome");

        // Wait for the Ping we're about to send.
        let event = handle
            .inbound
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("ping event");
        let (cid, ts) = match event {
            net::NetInbound::Message {
                client_id,
                msg: ClientMessage::Ping { timestamp_ms },
            } => (client_id, timestamp_ms),
            other => panic!("unexpected event: {other:?}"),
        };
        outbound
            .send(net::NetOutbound::ToClient {
                client_id: cid,
                msg: ServerMessage::Pong { timestamp_ms: ts },
            })
            .expect("send pong");
        client_id
    });

    // Client side: expect Welcome first.
    let welcome = read_one_server_message(&mut stream).await;
    let player_id = match welcome {
        ServerMessage::Welcome { player_id, .. } => player_id,
        other => panic!("expected Welcome, got {other:?}"),
    };
    assert!(player_id >= 1);

    // Send a Ping with a known timestamp.
    let ping = encode_client_frame(&ClientMessage::Ping {
        timestamp_ms: 424242,
    })
    .expect("encode ping");
    stream.write_all(&ping).await.expect("write ping");

    // Expect a matching Pong.
    let pong = read_one_server_message(&mut stream).await;
    match pong {
        ServerMessage::Pong { timestamp_ms } => assert_eq!(timestamp_ms, 424242),
        other => panic!("expected Pong, got {other:?}"),
    }

    // Wait for the server-side bookkeeping to finish.
    let _cid = welcome_task.await.expect("welcome task");
}
