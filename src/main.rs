use std::error::Error;

use futures::prelude::*;
use libp2p::{identity, Multiaddr, PeerId, ping, swarm::{keep_alive, NetworkBehaviour, SwarmBuilder, SwarmEvent}, tcp, Transport, yamux};
use libp2p::core::upgrade::Version;
use libp2p_test_handshake::TestHandshake;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {local_peer_id:?}");

    let transport = tcp::async_io::Transport::default()
        .upgrade(Version::V1Lazy)
        .authenticate(TestHandshake::new(local_key))
        .multiplex(yamux::Config::default())
        .boxed();

    let mut swarm =
        SwarmBuilder::with_async_std_executor(transport, Behaviour::default(), local_peer_id)
            .build();

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    if let Some(addr) = std::env::args().nth(1) {
        let remote: Multiaddr = addr.parse()?;
        swarm.dial(remote)?;
        println!("Dialed {addr}")
    }

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => println!("Listening on - {address:?}"),
            SwarmEvent::Behaviour(event) => println!("Got behavior event - {event:?}"),
            SwarmEvent::OutgoingConnectionError { error, .. } => panic!("Outgoing connection error: {error:?}"),
            SwarmEvent::IncomingConnectionError { error, .. } => panic!("Incoming connection error: {error:?}"),
            event => println!("Got event - {event:?}")
        }
    }
}

/// Our network behaviour.
///
/// For illustrative purposes, this includes the [`KeepAlive`](keep_alive::Behaviour) behaviour so a continuous sequence of
/// pings can be observed.
#[derive(NetworkBehaviour, Default)]
struct Behaviour {
    keep_alive: keep_alive::Behaviour,
    ping: ping::Behaviour,
}
