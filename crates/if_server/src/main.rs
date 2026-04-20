// if_server: Headless authoritative server for Infinite Flux.
//
// We run a minimal (headless) Bevy app for simulation and a tokio runtime on
// a dedicated OS thread for networking. The two sides talk via
// `tokio::sync::mpsc` + `tokio::sync::broadcast` channels. That keeps Bevy's
// scheduler free of any tokio-specific entanglement while still letting us
// use tokio's excellent TCP machinery for connection handling.

mod net;

use bevy::app::{App, ScheduleRunnerPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use std::net::SocketAddr;
use std::time::Duration;

use if_factory::FactoryPlugin;
use if_world::WorldPlugin;

use net::{NetHandle, NetInbound, NetOutbound, spawn_network_thread};

/// How often the headless simulation ticks. 60 Hz matches a typical Bevy
/// `FixedUpdate` rate and keeps the server responsive without melting CPU.
const TICK_HZ: f64 = 60.0;

/// Address the TCP listener binds to. Loopback only for now; config comes later.
const BIND_ADDR: &str = "127.0.0.1:7777";

fn main() {
    // Parse the bind address once so a typo fails fast at startup rather than
    // somewhere deep inside the network thread.
    let bind_addr: SocketAddr = BIND_ADDR
        .parse()
        .expect("hard-coded BIND_ADDR must parse as SocketAddr");

    // Spin up the tokio-backed networking layer on a dedicated thread.
    let net_handle = spawn_network_thread(bind_addr);

    App::new()
        // Headless: no windowing, no rendering. `MinimalPlugins` gives us the
        // core scheduler; `ScheduleRunnerPlugin` drives the loop at a fixed
        // rate instead of relying on a window's vsync.
        .add_plugins(
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / TICK_HZ,
            ))),
        )
        // `StatesPlugin` is required by the bevy_state feature used elsewhere
        // in the workspace; cheap to include and keeps parity with the client.
        .add_plugins(StatesPlugin)
        .add_plugins(bevy::log::LogPlugin::default())
        // Simulation plugins — same ones the client uses.
        .add_plugins(WorldPlugin)
        .add_plugins(FactoryPlugin)
        // Networking bridge resource.
        .insert_resource(net_handle)
        .add_systems(Startup, log_startup)
        .add_systems(Update, drain_network_inbound)
        .run();
}

fn log_startup(net: Res<NetHandle>) {
    info!("if_server online, listening on {}", net.bind_addr);
    info!("tick rate: {:.0} Hz", TICK_HZ);
}

/// Pull inbound messages from the network thread into Bevy-land. For the
/// foundation we mostly react to chat and pings; game actions are logged so we
/// can see the pipeline working end-to-end.
fn drain_network_inbound(net: Res<NetHandle>) {
    // Acquire the receiver lock once and drain. Only this system touches it,
    // so contention is effectively zero.
    let inbound = net.inbound.lock().expect("inbound receiver poisoned");
    // try_recv in a loop — we never want to block the Bevy scheduler.
    while let Ok(event) = inbound.try_recv() {
        match event {
            NetInbound::Connected { client_id } => {
                info!("client {client_id} connected");
                // Send a welcome with an empty snapshot. Snapshot contents
                // will be filled in once we wire the simulation state up.
                let welcome = if_protocol::ServerMessage::Welcome {
                    player_id: client_id,
                    snapshot: if_protocol::WorldSnapshot::default(),
                };
                let _ = net.outbound.send(NetOutbound::ToClient {
                    client_id,
                    msg: welcome,
                });
            }
            NetInbound::Disconnected { client_id } => {
                info!("client {client_id} disconnected");
            }
            NetInbound::Message { client_id, msg } => {
                handle_client_message(&net, client_id, msg);
            }
        }
    }
}

fn handle_client_message(net: &NetHandle, client_id: u64, msg: if_protocol::ClientMessage) {
    use if_protocol::{ClientMessage, ServerMessage};
    match msg {
        ClientMessage::Hello { player_name } => {
            info!("client {client_id} hello: {player_name}");
        }
        ClientMessage::PlaceBuilding { pos, building_type } => {
            info!(
                "client {client_id} place building: pos=({},{}) type={}",
                pos.x, pos.y, building_type
            );
            // TODO: apply to simulation + broadcast EntityUpdate::BuildingPlaced.
        }
        ClientMessage::RemoveBuilding { pos } => {
            info!(
                "client {client_id} remove building at ({},{})",
                pos.x, pos.y
            );
            // TODO: apply to simulation + broadcast EntityUpdate::BuildingRemoved.
        }
        ClientMessage::Chat { text } => {
            info!("chat from {client_id}: {text}");
            let broadcast = ServerMessage::Chat {
                from: format!("player{client_id}"),
                text,
            };
            let _ = net.outbound.send(NetOutbound::Broadcast { msg: broadcast });
        }
        ClientMessage::Ping { timestamp_ms } => {
            let pong = ServerMessage::Pong { timestamp_ms };
            let _ = net.outbound.send(NetOutbound::ToClient {
                client_id,
                msg: pong,
            });
        }
    }
}
