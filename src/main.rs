use futures::{prelude::stream::StreamExt};
use libp2p::{
   
    identity::Keypair,
    swarm::SwarmEvent,
    PeerId, Swarm,
    ping::{Ping, PingEvent, PingConfig}, Multiaddr
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    KeepAliveMessage,
}

pub struct Peer {
    swarm: Swarm<Ping>,
}

impl Peer {
    pub async fn new(
    ) -> Self {
        // Create a random PeerId
        let local_key = Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        println!("Local peer id {:?}", local_peer_id);

        // Set up an encrypted DNS-enabled TCP Transport
        let transport = libp2p::development_transport(local_key).await.unwrap();

        // Create a Swarm to manage peers and events
        let local_peer = Self {
            swarm: {
                let behaviour = Ping::new(PingConfig::new().with_keep_alive(true));
                Swarm::new(transport, behaviour, local_peer_id)
            },
        };
        local_peer
    }

    pub fn listen_for_dialing(&mut self) {
        self.swarm
            .listen_on(
                "/ip4/0.0.0.0/tcp/0"
                    .parse()
                    .expect("can get a local socket"),
            )
            .expect("swarm can be started");
        if let Some(remote_peer) = std::env::args().nth(1) {
            let remote_peer_multiaddr: Multiaddr = remote_peer.parse().expect("valid address");
            self.swarm.dial(remote_peer_multiaddr).expect("known peer");
            println!("Dialed remote peer: {:?}", remote_peer);
        }
    }

    pub fn match_event<T>(&mut self, event: SwarmEvent<PingEvent, T>) {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {:?}", address);
            }
            SwarmEvent::Behaviour(event) => {
                println!("{:?}", event)
            }
            _ => {
                // println!("Unhandled swarm event");
            }
        }
    }
}

use futures::{
    select,
};


#[async_std::main]
async fn main() {

    let mut my_peer = Peer::new().await;

    // Listen on all interfaces and whatever port the OS assigns
    my_peer.listen_for_dialing();

    // Process events
    loop {
        select! {
            event = my_peer.swarm.select_next_some() => my_peer.match_event(event),
        }
    }
}