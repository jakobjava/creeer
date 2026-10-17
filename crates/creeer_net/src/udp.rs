//! The game over UDP: netcode's transport under renet, for a dedicated server and the
//! clients that join it.
//!
//! Connections are unsecured for now (a client names its own id; nothing is encrypted with
//! a key the server hands out): enough for playing on a LAN or with friends, not for a
//! public server, which needs a token service (ADR-0008).

use crate::PROTOCOL_VERSION;
use crate::channel::config;
use renet::{ClientId, RenetClient, RenetServer};
use renet_netcode::{
    ClientAuthentication, NetcodeClientTransport, NetcodeError, NetcodeServerTransport,
    NetcodeTransportError, ServerAuthentication, ServerConfig,
};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// What netcode tells this game's packets by ("CREEER" and the protocol version): a
/// server and client of different versions do not connect.
pub const PROTOCOL_ID: u64 = 0x4352_4545_4552_0000 | PROTOCOL_VERSION as u64;

/// The players a server takes at most.
pub const MOST_CLIENTS: usize = 32;

/// The time netcode counts in: since the Unix epoch.
pub fn now() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

/// A server listening on `addr` (port 0: any free one).
///
/// # Errors
/// If the socket cannot be bound.
pub fn listen(addr: SocketAddr) -> std::io::Result<(RenetServer, NetcodeServerTransport)> {
    let socket = UdpSocket::bind(addr)?;
    let public = socket.local_addr()?;
    let transport = NetcodeServerTransport::new(
        ServerConfig {
            current_time: now(),
            max_clients: MOST_CLIENTS,
            protocol_id: PROTOCOL_ID,
            public_addresses: vec![public],
            authentication: ServerAuthentication::Unsecure,
        },
        socket,
    )?;
    Ok((RenetServer::new(config()), transport))
}

/// A client id not likely to be anyone else's: the clock's nanoseconds mixed with the
/// process's id.
pub fn fresh_id() -> ClientId {
    let t = u64::try_from(now().as_nanos() & u128::from(u64::MAX)).unwrap_or_default();
    let mut x = t ^ (u64::from(std::process::id()) << 32);
    // splitmix64's finaliser
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// A client connecting to the server at `server` as `id`.
///
/// # Errors
/// If no socket can be bound, or netcode refuses the setup.
pub fn connect(
    server: SocketAddr,
    id: ClientId,
) -> Result<(RenetClient, NetcodeClientTransport), NetcodeTransportError> {
    let any = if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let socket = UdpSocket::bind(any).map_err(NetcodeTransportError::IO)?;
    let authentication = ClientAuthentication::Unsecure {
        protocol_id: PROTOCOL_ID,
        client_id: id,
        server_addr: server,
        user_data: None,
    };
    let transport = NetcodeClientTransport::new(now(), authentication, socket)
        .map_err(|e: NetcodeError| NetcodeTransportError::Netcode(e))?;
    Ok((RenetClient::new(config()), transport))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::RELIABLE;

    #[test]
    fn a_client_connects_over_loopback_and_messages_go_both_ways() {
        let (mut server, mut server_net) = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = server_net.addresses()[0];
        let (mut client, mut client_net) = connect(addr, 42).unwrap();
        let step = Duration::from_millis(5);
        let (mut heard, mut answered) = (false, false);
        for _ in 0..1000 {
            server.update(step);
            server_net.update(step, &mut server).unwrap();
            client.update(step);
            client_net.update(step, &mut client).unwrap();
            if client.is_connected() && !heard {
                client.send_message(RELIABLE, b"hello".to_vec());
            }
            if let Some(m) = server.receive_message(42, RELIABLE) {
                assert_eq!(&m[..], b"hello");
                heard = true;
                server.send_message(42, RELIABLE, b"welcome".to_vec());
            }
            if let Some(m) = client.receive_message(RELIABLE) {
                assert_eq!(&m[..], b"welcome");
                answered = true;
                break;
            }
            client_net.send_packets(&mut client).unwrap();
            server_net.send_packets(&mut server);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(heard && answered, "heard {heard}, answered {answered}");
    }
}
