use async_std::task;
use futures::{prelude::stream::StreamExt, stream::SelectNextSome};
use libp2p::{
    floodsub::{Floodsub, FloodsubEvent, Topic},
    identity::Keypair,
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    multiaddr::Protocol,
    multihash::Multihash,
    swarm::SwarmEvent,
    NetworkBehaviour, PeerId, Swarm,
};
use serde::{Deserialize, Serialize};

// We create a custom network behaviour that combines floodsub and mDNS.
// Use the derive to generate delegating NetworkBehaviour impl.
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "OutEvent")]
pub struct P2PBehaviour {
    floodsub: Floodsub,
    mdns: Mdns,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum OutEvent {
    Floodsub(FloodsubEvent),
    Mdns(MdnsEvent),
}

impl From<MdnsEvent> for OutEvent {
    fn from(v: MdnsEvent) -> Self {
        Self::Mdns(v)
    }
}

impl From<FloodsubEvent> for OutEvent {
    fn from(v: FloodsubEvent) -> Self {
        Self::Floodsub(v)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    KeepAliveMessage,
}

pub struct Peer {
    id: PeerId,
    floodsub_topic: Topic,
    swarm: Swarm<P2PBehaviour>,
}

impl Peer {
    pub async fn new(
        topic: &str,
    ) -> Self {
        // Create a random PeerId
        let local_key = Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());

        // Set up an encrypted DNS-enabled TCP Transport
        let transport = libp2p::development_transport(local_key).await.unwrap();

        // Create a Floodsub topic
        let floodsub_topic = Topic::new(topic);

        // Create a Swarm to manage peers and events
        let local_peer = Self {
            id: local_peer_id,
            floodsub_topic: floodsub_topic.clone(),
            swarm: {
                let mdns = task::block_on(Mdns::new(MdnsConfig::default())).unwrap();
                let mut behaviour = P2PBehaviour {
                    floodsub: Floodsub::new(local_peer_id),
                    mdns,
                };

                behaviour.floodsub.subscribe(floodsub_topic);
                Swarm::new(transport, behaviour, local_peer_id)
            },
        };
        // println!(
        //     "Local node initialized with number: {} and peer id: {:?}",
        //     local_peer.replica_number, local_peer_id
        // );
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
    }

    pub fn broadcast_message(&mut self) {
        self.swarm.behaviour_mut().floodsub.publish(
            self.floodsub_topic.clone(),
            serde_json::to_string::<Message>(&Message::KeepAliveMessage).unwrap(),
        );
    }

    pub fn get_next_event(&mut self) -> SelectNextSome<'_, Swarm<P2PBehaviour>> {
        self.swarm.select_next_some()
    }

    pub fn match_event<T>(&mut self, event: SwarmEvent<OutEvent, T>) {
        match event {
            SwarmEvent::NewListenAddr { mut address, .. } => {
                address.push(Protocol::P2p(
                    Multihash::from_bytes(&self.id.to_bytes()[..]).unwrap(),
                ));
                println!("Listening on {:?}", address);
            }
            SwarmEvent::Behaviour(OutEvent::Floodsub(FloodsubEvent::Message(floodsub_message))) => {
                let floodsub_content = String::from_utf8_lossy(&floodsub_message.data);
                let message =
                    serde_json::from_str::<Message>(&floodsub_content).expect("can parse artifact");
                self.handle_incoming_message(message);
            }
            SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Discovered(list))) => {
                for (peer, _) in list {
                    self.swarm
                        .behaviour_mut()
                        .floodsub
                        .add_node_to_partial_view(peer);
                    println!("Discovered peer {:?}", peer);
                }
            }
            SwarmEvent::Behaviour(OutEvent::Mdns(MdnsEvent::Expired(list))) => {
                for (peer, _) in list {
                    if !self.swarm.behaviour_mut().mdns.has_node(&peer) {
                        self.swarm
                            .behaviour_mut()
                            .floodsub
                            .remove_node_from_partial_view(&peer);
                    }
                }
                // println!("Ignoring Mdns expired event");
            }
            _ => {
                // println!("Unhandled swarm event");
            }
        }
    }

    pub fn handle_incoming_message(&mut self, message_variant: Message) {
        match message_variant {
            Message::KeepAliveMessage => println!("Received keep alive message"),
        }
    }
}

use async_std::task::sleep;
use futures::{
    future::FutureExt,
    select,
};
use std::{
    time::Duration,
};



async fn broadcast_message_future() {
    sleep(Duration::from_millis(1000)).await;
}

#[async_std::main]
async fn main() {

    let mut my_peer = Peer::new(
        "gossip_blocks",
    )
    .await;

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
            event = my_peer.get_next_event() => my_peer.match_event(event),
        }
    }
}