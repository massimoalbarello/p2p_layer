use std::{time::Duration, collections::BTreeSet};

use futures::{prelude::stream::StreamExt, FutureExt};
use async_std::task::sleep;

use libp2p::{
   
    identity::Keypair,
    swarm::SwarmEvent,
    PeerId, Swarm,
    floodsub::{Floodsub, FloodsubEvent, Topic},
    Multiaddr
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    KeepAliveMessage,
}

pub struct Peer {
    local_peer_id: PeerId,
    swarm: Swarm<Floodsub>,
    floodsub_topic: Topic,
    subscribed_peers: BTreeSet<PeerId>,
}

impl Peer {
    pub async fn new(
    ) -> Self {
        // Create a random PeerId
        let local_key = Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());

        // Set up an encrypted DNS-enabled TCP Transport
        let transport = libp2p::development_transport(local_key).await.unwrap();

        // Create a Floodsub topic
        let floodsub_topic = Topic::new("topic");

        // Create a Swarm to manage peers and events
        let local_peer = Self {
            local_peer_id,
            swarm: {
                let mut behaviour = Floodsub::new(local_peer_id);
                behaviour.subscribe(floodsub_topic.clone());
                Swarm::new(transport, behaviour, local_peer_id)
            },
            floodsub_topic,
            subscribed_peers: BTreeSet::new(),
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
            self.swarm.dial(remote_peer_multiaddr.clone()).expect("known peer");
            println!("Dialed remote peer: {:?}", remote_peer);
            let remote_peer_id = PeerId::try_from_multiaddr(&remote_peer_multiaddr).expect("multiaddress with peer ID");
            self.swarm
                .behaviour_mut()
                .add_node_to_partial_view(remote_peer_id);
            println!("Added peer with ID: {:?} to broadcast list", remote_peer_id);
        }
    }

    pub fn match_event<T>(&mut self, event: SwarmEvent<FloodsubEvent, T>) {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("{}", format!("{}/p2p/{}", address, self.local_peer_id.to_string()));
            }
            SwarmEvent::Behaviour(event) => {
                match event {
                    FloodsubEvent::Subscribed { peer_id, .. } => {
                        if !self.subscribed_peers.contains(&peer_id) {
                            self.swarm
                                .behaviour_mut()
                                .add_node_to_partial_view(peer_id);
                            self.subscribed_peers.insert(peer_id);
                            println!("Added peer with ID: {:?} to broadcast list", peer_id);
                        }
                    },
                    FloodsubEvent::Message(message) => {
                        println!("{:?}", String::from_utf8(message.data).expect("data as Vec<u8>"));
                    },
                    _ => println!("{:?}", event), 
                }
            }
            _ => {
                // println!("Unhandled swarm event");
            }
        }
    }

    pub fn broadcast_message(&mut self) {
        self.swarm.behaviour_mut().publish(
            self.floodsub_topic.clone(),
            "ciao!"
        );
    }
}

use futures::{
    select,
};


async fn broadcast_message_future() {
    sleep(Duration::from_millis(100)).await;
}

#[async_std::main]
async fn main() {

    let mut my_peer = Peer::new().await;

    // Listen on all interfaces and whatever port the OS assigns
    my_peer.listen_for_dialing();

    // Process events
    loop {
        select! {
            _ = broadcast_message_future().fuse() => {
                // prevent Mdns expiration event by periodically broadcasting keep alive messages to peers
                // if any locally generated artifact, broadcast it
                my_peer.broadcast_message();
            },
            event = my_peer.swarm.select_next_some() => my_peer.match_event(event),
        }
    }
}