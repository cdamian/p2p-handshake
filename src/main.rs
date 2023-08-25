extern crate core;

use std::error::Error;
use std::pin::Pin;

use asynchronous_codec::{Decoder, Encoder, Framed, FramedParts};
use futures::prelude::*;
use libp2p::{identity, InboundUpgrade, Multiaddr, OutboundUpgrade, PeerId, ping, swarm::{keep_alive, NetworkBehaviour, SwarmBuilder, SwarmEvent}, tcp, Transport, yamux};
use libp2p::bytes::BytesMut;
use libp2p::core::upgrade::Version;
use libp2p::core::UpgradeInfo;
use libp2p::identity::{Keypair, PublicKey};
use unsigned_varint::codec::UviBytes;

#[derive(Clone)]
struct TestHandshake {
    identity: Keypair,
}

impl TestHandshake {
    fn new(identity: Keypair) -> Self {
        TestHandshake { identity }
    }

    async fn send_handshake_info<T, U>(&self, framed_socket: &mut Framed<T, U>) -> Result<(), TestHandshakeError>
        where
            T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
            U: Encoder<Item=BytesMut>,
    {
        // Send public key.
        let encoded_key = self.identity.public().encode_protobuf();

        framed_socket.send(BytesMut::from(encoded_key.as_slice()))
            .await
            .map_err(|_| TestHandshakeError::SendError)?;

        // Send signature.
        let local_peer_id = PeerId::from(self.identity.public());

        let sig = self.identity.sign(local_peer_id.to_bytes().as_slice())
            .map_err(|_| TestHandshakeError::SigningError)?;

        framed_socket.send(BytesMut::from(sig.as_slice()))
            .await
            .map_err(|_| TestHandshakeError::SendError)?;

        Ok(())
    }

    async fn receive_handshake_info<T, U>(&self, framed_socket: &mut Framed<T, U>) -> Result<(PublicKey, PeerId, BytesMut), TestHandshakeError>
        where
            T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
            U: Decoder<Item=BytesMut>,
    {
        // Receive public key.
        let rec = framed_socket.next()
            .await
            .ok_or(TestHandshakeError::AwaitError)?
            .map_err(|_| TestHandshakeError::ReceiveError)?;

        let remote_public_key = PublicKey::try_decode_protobuf(&rec)
            .map_err(|_| TestHandshakeError::KeyDecodeError)?;

        let remote_peer_id = PeerId::from(&remote_public_key);

        // Receive signature.
        let sig = framed_socket.next()
            .await
            .ok_or(TestHandshakeError::AwaitError)?
            .map_err(|_| TestHandshakeError::ReceiveError)?;

        Ok((remote_public_key, remote_peer_id, sig))
    }
}

const PROTOCOL_NAME: &str = "/test-handshake";

impl UpgradeInfo for TestHandshake {
    type Info = &'static str;
    type InfoIter = std::iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once(PROTOCOL_NAME)
    }
}

impl<T> InboundUpgrade<T> for TestHandshake
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (PeerId, T);
    type Error = TestHandshakeError;
    type Future = Pin<Box<dyn Future<Output=Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: T, _: Self::Info) -> Self::Future {
        async move {
            let mut framed_socket = Framed::new(socket, UviBytes::default());

            self.send_handshake_info(&mut framed_socket).await?;

            let (remote_public_key, remote_peer_id, sig) = self.receive_handshake_info(&mut framed_socket).await?;

            if !remote_public_key.verify(remote_peer_id.to_bytes().as_slice(), &sig) {
                return Err(TestHandshakeError::SignatureError);
            }

            let FramedParts { io, .. } = framed_socket.into_parts();

            Ok((remote_peer_id, io))
        }.boxed()
    }
}

impl<T> OutboundUpgrade<T> for TestHandshake
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (PeerId, T);
    type Error = TestHandshakeError;
    type Future = Pin<Box<dyn Future<Output=Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: T, _: Self::Info) -> Self::Future {
        async move {
            let mut framed_socket = Framed::new(socket, UviBytes::default());

            let (remote_public_key, remote_peer_id, sig) = self.receive_handshake_info(&mut framed_socket).await?;

            if !remote_public_key.verify(remote_peer_id.to_bytes().as_slice(), &sig) {
                return Err(TestHandshakeError::SignatureError);
            }

            self.send_handshake_info(&mut framed_socket).await?;

            let FramedParts { io, .. } = framed_socket.into_parts();

            Ok((remote_peer_id, io))
        }.boxed()
    }
}

#[derive(Debug, thiserror::Error)]
enum TestHandshakeError {
    #[error("Send error")]
    SendError,
    #[error("Receive error")]
    ReceiveError,
    #[error("Await error")]
    AwaitError,
    #[error("Signing error")]
    SigningError,
    #[error("Signature error")]
    SignatureError,
    #[error("Key decode error")]
    KeyDecodeError,
}

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

    // Tell the swarm to listen on all interfaces and a random, OS-assigned
    // port.
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // Dial the peer identified by the multi-address given as the second
    // command-line argument, if any.
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
