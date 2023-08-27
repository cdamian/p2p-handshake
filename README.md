# P2P Handshake

A small tool based of
the [libp2p ping example](https://github.com/libp2p/rust-libp2p/tree/master/examples/ping-example),
that is using the test handshake config found
in [libp2p-test-handshake](https://github.com/cdamian/libp2p-test-handshake).

The tool will panic if any Incoming/Outgoing connection errors are encountered, otherwise, it will print out the ping
events.

## Usage

1. In a first terminal window, run the following command:

   ```sh
   cargo run
   ```

   This command starts a node and prints the `PeerId` and the listening addresses, such
   as `Listening on "/ip4/0.0.0.0/tcp/24915"`.

2. In a second terminal window, start a new instance of the example with the following command:

   ```sh
   cargo run -- /ip4/127.0.0.1/tcp/24915
   ```

   Replace `/ip4/127.0.0.1/tcp/24915` with the listen address of the first node obtained from the first terminal window.

3. The two nodes will establish a connection, negotiate the ping protocol, and begin pinging each other.
