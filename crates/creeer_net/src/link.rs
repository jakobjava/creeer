//! A simulated network between a server and a client in one process: renet's packets held
//! back by a latency (and a random jitter, so some overtake others) and some lost, every
//! byte counted as it would be on the wire. For testing the game over a bad link, and for
//! measuring what a client is sent.
//!
//! A singleplayer game runs its server on a thread of its own, joined to the client by a
//! [`memory`] connection: the same renet connection, its packets handed over at once.

use crate::channel::{PACKET_OVERHEAD, config};
use renet::{ClientId, ClientNotFound, RenetClient, RenetServer};
use std::time::Duration;

/// How bad the link is, each way.
#[derive(Clone, Copy, Debug, Default)]
pub struct Conditions {
    /// The delay every packet has.
    pub latency: Duration,
    /// The most a packet is delayed beyond it (uniformly at random).
    pub jitter: Duration,
    /// The fraction of packets lost.
    pub loss: f64,
}

/// One direction: packets on their way, with when they arrive.
#[derive(Debug, Default)]
struct Way {
    flying: Vec<(Duration, Vec<u8>)>,
    bytes: u64,
    packets: u64,
}

/// The link.
#[derive(Debug)]
pub struct SimulatedLink {
    /// Its conditions (may be changed as it runs).
    pub conditions: Conditions,
    seed: u64,
    to_client: Way,
    to_server: Way,
}

/// A client on a simulated link: a connection made on the server for `id`, and the client's
/// end of it.
pub fn connect(server: &mut RenetServer, id: ClientId) -> RenetClient {
    let mut client = RenetClient::new(config());
    client.set_connected();
    server.add_connection(id);
    client
}

impl SimulatedLink {
    /// A link with `conditions`, its randomness from `seed`.
    pub fn new(conditions: Conditions, seed: u64) -> Self {
        Self {
            conditions,
            seed: seed | 1,
            to_client: Way::default(),
            to_server: Way::default(),
        }
    }

    fn random(&mut self) -> f64 {
        // xorshift64*
        self.seed ^= self.seed >> 12;
        self.seed ^= self.seed << 25;
        self.seed ^= self.seed >> 27;
        #[allow(clippy::cast_precision_loss)] // 53 bits
        let u = (self.seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64 / (1u64 << 53) as f64;
        u
    }

    fn launch(&mut self, now: Duration, packets: Vec<Vec<u8>>, to_client: bool) {
        for packet in packets {
            let way = if to_client {
                &mut self.to_client
            } else {
                &mut self.to_server
            };
            way.bytes += (packet.len() + PACKET_OVERHEAD) as u64;
            way.packets += 1;
            if self.random() < self.conditions.loss {
                continue;
            }
            let late = self.conditions.jitter.mul_f64(self.random());
            let at = now + self.conditions.latency + late;
            let way = if to_client {
                &mut self.to_client
            } else {
                &mut self.to_server
            };
            way.flying.push((at, packet));
        }
    }

    /// At `now`: sends what each end has to send, and hands each what has arrived.
    ///
    /// # Errors
    /// If the server has no connection for `id`.
    pub fn exchange(
        &mut self,
        now: Duration,
        server: &mut RenetServer,
        id: ClientId,
        client: &mut RenetClient,
    ) -> Result<(), ClientNotFound> {
        let down = server.get_packets_to_send(id)?;
        self.launch(now, down, true);
        let up = client.get_packets_to_send();
        self.launch(now, up, false);
        for packet in arrived(&mut self.to_client, now) {
            client.process_packet(&packet);
        }
        for packet in arrived(&mut self.to_server, now) {
            server.process_packet_from(&packet, id)?;
        }
        Ok(())
    }

    /// The bytes sent to the client so far, on the wire (lost ones too), and the packets.
    pub fn sent_to_client(&self) -> (u64, u64) {
        (self.to_client.bytes, self.to_client.packets)
    }

    /// The bytes and packets sent to the server so far.
    pub fn sent_to_server(&self) -> (u64, u64) {
        (self.to_server.bytes, self.to_server.packets)
    }
}

/// One end of an in-process connection: renet's packets handed to the other end through a
/// channel, at once (a singleplayer game's server on its own thread and the client on the
/// window's). Either end may be on any thread.
#[derive(Debug)]
pub struct MemoryEnd {
    to: std::sync::mpsc::Sender<Vec<u8>>,
    from: std::sync::mpsc::Receiver<Vec<u8>>,
    closed: std::cell::Cell<bool>,
}

/// The two ends of an in-process connection.
pub fn memory() -> (MemoryEnd, MemoryEnd) {
    let (a_to, b_from) = std::sync::mpsc::channel();
    let (b_to, a_from) = std::sync::mpsc::channel();
    (
        MemoryEnd {
            to: a_to,
            from: a_from,
            closed: std::cell::Cell::new(false),
        },
        MemoryEnd {
            to: b_to,
            from: b_from,
            closed: std::cell::Cell::new(false),
        },
    )
}

impl MemoryEnd {
    /// Sends packets; false if the other end is gone.
    pub fn send(&self, packets: Vec<Vec<u8>>) -> bool {
        packets.into_iter().all(|p| self.to.send(p).is_ok())
    }

    /// The packets that came.
    pub fn receive(&self) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        loop {
            match self.from.try_recv() {
                Ok(packet) => packets.push(packet),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.closed.set(true);
                    break;
                }
            }
        }
        packets
    }

    /// Whether the other end is gone (known once it was received from).
    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }

    /// Sends what `client` has to send and hands it what came; false if the other end is
    /// gone.
    pub fn exchange_client(&self, client: &mut RenetClient) -> bool {
        for packet in self.receive() {
            client.process_packet(&packet);
        }
        self.send(client.get_packets_to_send())
    }

    /// The same for the server's connection `id`; false if the other end is gone.
    ///
    /// # Errors
    /// If the server has no connection for `id`.
    pub fn exchange_server(
        &self,
        server: &mut RenetServer,
        id: ClientId,
    ) -> Result<bool, ClientNotFound> {
        for packet in self.receive() {
            server.process_packet_from(&packet, id)?;
        }
        Ok(self.send(server.get_packets_to_send(id)?))
    }
}

/// A simulated link between two in-process connections: the server's end of one, the
/// client's of the other, and the packets carried between them as [`SimulatedLink`] carries
/// them. For playing a whole game (its server and client as they are) over a bad network.
#[derive(Debug)]
pub struct Relay {
    /// The link (its conditions may be changed as it runs, its bytes counted).
    pub link: SimulatedLink,
    server: MemoryEnd,
    client: MemoryEnd,
}

impl Relay {
    /// Carries what `server` (the client's end of a server's in-process connection) sends
    /// and is sent over `link`; returns the relay and the end for the client.
    pub fn new(server: MemoryEnd, link: SimulatedLink) -> (Self, MemoryEnd) {
        let (relay_end, client_end) = memory();
        (
            Self {
                link,
                server,
                client: relay_end,
            },
            client_end,
        )
    }

    /// At `now`: takes in what either side sent and hands over what has arrived; false if
    /// either side is gone.
    pub fn pump(&mut self, now: Duration) -> bool {
        let down = self.server.receive();
        self.link.launch(now, down, true);
        let up = self.client.receive();
        self.link.launch(now, up, false);
        let to_client = arrived(&mut self.link.to_client, now);
        let to_server = arrived(&mut self.link.to_server, now);
        self.client.send(to_client) && self.server.send(to_server)
    }
}
fn arrived(way: &mut Way, now: Duration) -> Vec<Vec<u8>> {
    let (mut here, flying): (Vec<_>, Vec<_>) = std::mem::take(&mut way.flying)
        .into_iter()
        .partition(|(at, _)| *at <= now);
    way.flying = flying;
    here.sort_by_key(|(at, _)| *at);
    here.into_iter().map(|(_, packet)| packet).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{RELIABLE, UNRELIABLE};

    #[test]
    fn reliable_messages_arrive_in_order_over_a_bad_link_and_unreliable_ones_mostly() {
        let mut server = RenetServer::new(config());
        let mut client = connect(&mut server, 7);
        let conditions = Conditions {
            latency: Duration::from_millis(60),
            jitter: Duration::from_millis(40),
            loss: 0.2,
        };
        let mut link = SimulatedLink::new(conditions, 11);
        let (mut reliable, mut unreliable) = (Vec::new(), 0);
        let step = Duration::from_millis(16);
        for k in 0..400u32 {
            let now = step * k;
            if k < 200 {
                server.send_message(7, RELIABLE, k.to_le_bytes().to_vec());
                server.send_message(7, UNRELIABLE, vec![0u8; 300]);
            }
            server.update(step);
            client.update(step);
            link.exchange(now, &mut server, 7, &mut client).unwrap();
            while let Some(m) = client.receive_message(RELIABLE) {
                reliable.push(u32::from_le_bytes(m[..4].try_into().unwrap()));
            }
            while client.receive_message(UNRELIABLE).is_some() {
                unreliable += 1;
            }
        }
        assert_eq!(reliable, (0..200).collect::<Vec<_>>());
        // A fifth lost, and those that came together in one packet lost together.
        assert!((120..=180).contains(&unreliable), "{unreliable} of 200");
        let (bytes, packets) = link.sent_to_client();
        assert!(
            bytes > 200 * 300 && packets >= 200,
            "{bytes} bytes in {packets} packets"
        );
    }
}
