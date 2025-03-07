use iced::futures::channel::mpsc;
use iced::futures::{select, SinkExt};
use libp2p::{kad, mdns, noise, Swarm, SwarmBuilder, tcp, yamux};
use libp2p::futures::StreamExt;
use libp2p::kad::Mode;
use libp2p::kad::store::MemoryStore;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use tracing::{error, info};
use crate::SwarmCommand;

#[derive(NetworkBehaviour)]
struct CustomBehaviour {
    kademlia: kad::Behaviour<MemoryStore>,
    mdns: mdns::tokio::Behaviour,
}

pub async fn run(mut commands: mpsc::Receiver<SwarmCommand>, mut events: mpsc::Sender<crate::SwarmEvent>) {
    let mut swarm: Swarm<CustomBehaviour> = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .expect("Failed to build tcp config")
        .with_quic()
        .with_behaviour(|key| {
            Ok(CustomBehaviour {
                kademlia: kad::Behaviour::new(
                    key.public().to_peer_id(),
                    MemoryStore::new(key.public().to_peer_id()),
                ),
                mdns: mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    key.public().to_peer_id(),
                )
                    .expect("Failed to set up mDNS behaviour"),
            })
        })
        .expect("Failed to build Swarm")
        .build();

    swarm.behaviour_mut().kademlia.set_mode(Some(Mode::Server));

    swarm
        .listen_on("/ip4/0.0.0.0/tcp/0".parse().expect("Failed to parse multiaddress"))
        .expect("Failed to start a Swarm");

    loop {
        select! {
            cmd = commands.select_next_some() => handle_command(cmd, &mut swarm).await,
            event = swarm.select_next_some() => handle_swarm_event(event, &mut swarm, &mut events).await
        }
    }
}

async fn handle_command(cmd: SwarmCommand, swarm: &mut Swarm<CustomBehaviour>) {
    
}

async fn handle_swarm_event(event: SwarmEvent<CustomBehaviourEvent>, swarm: &mut Swarm<CustomBehaviour>,  sender: &mut mpsc::Sender<crate::SwarmEvent>) {
    match event {
        SwarmEvent::NewListenAddr {address, ..} => {
            info!("Listening in {address:?}");
        }
        SwarmEvent::Behaviour(CustomBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
            for (peer_id, multiaddr) in list {
                info!("Discovered peer {peer_id} at {multiaddr}");
                swarm.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);
            }
        }
        SwarmEvent::Behaviour(CustomBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
            for (peer_id, multiaddr) in list {
                info!("Expired peer {peer_id} at {multiaddr}");
                swarm.behaviour_mut().kademlia.remove_address(&peer_id, &multiaddr);
            }
        }
        SwarmEvent::Behaviour(CustomBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {result, ..})) => {
            match result {
                kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {key, providers})) => {
                    for peer in providers {
                        info!(
                                    "Peer {peer} provides key {}",
                                    std::str::from_utf8(key.as_ref()).unwrap()
                                );
                    }
                }
                kad::QueryResult::GetProviders(Err(err)) => {
                    error!("Failed to get providers: {err:?}");
                }
                kad::QueryResult::GetRecord(Ok(
                                                kad::GetRecordOk::FoundRecord(kad::PeerRecord {
                                                                                  record: kad::Record { key, value, .. },
                                                                                  ..
                                                                              })
                                            )) => {
                    info!(
                                "Got record {} : {}",
                                std::str::from_utf8(key.as_ref()).unwrap(),
                                std::str::from_utf8(&value).unwrap(),
                            );
                }
                kad::QueryResult::GetRecord(Ok(_)) => {}
                kad::QueryResult::GetRecord(Err(err)) => {
                    error!("Failed to get record: {err:?}");
                }
                kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
                    info!(
                                "Successfully put record {}",
                                std::str::from_utf8(key.as_ref()).unwrap()
                            );
                }
                kad::QueryResult::PutRecord(Err(err)) => {
                    info!("Failed to put record: {err:?}");
                }
                kad::QueryResult::StartProviding(Ok(kad::AddProviderOk { key })) => {
                    info!(
                                "Successfully put provider record {}",
                                std::str::from_utf8(key.as_ref()).unwrap()
                            );
                }
                kad::QueryResult::StartProviding(Err(err)) => {
                    error!("Failed to put provider record: {err:?}");
                }
                _ => {}
            }
        }
        _ => {}
    }
    
    let _ = sender.send(event).await;
}

pub fn handle_input_line(kademlia: &mut kad::Behaviour<MemoryStore>, line: String) {
    let mut args = line.split(' ');

    match args.next() {
        Some("GET") => {
            let key = {
                match args.next() {
                    Some(key) => kad::RecordKey::new(&key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };
            kademlia.get_record(key);
        }
        Some("GET_PROVIDERS") => {
            let key = {
                match args.next() {
                    Some(key) => kad::RecordKey::new(&key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };
            kademlia.get_providers(key);
        }
        Some("PUT") => {
            let key = {
                match args.next() {
                    Some(key) => kad::RecordKey::new(&key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };
            let value = {
                match args.next() {
                    Some(value) => value.as_bytes().to_vec(),
                    None => {
                        error!("Expected value");
                        return;
                    }
                }
            };
            let record = kad::Record {
                key,
                value,
                publisher: None,
                expires: None,
            };
            kademlia
                .put_record(record, kad::Quorum::Majority)
                .expect("Failed to store record.");
        }
        Some("PUT_PROVIDER") => {
            let key = {
                match args.next() {
                    Some(key) => kad::RecordKey::new(&key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };

            kademlia
                .start_providing(key)
                .expect("Failed to start providing key");
        }
        _ => {
            error!("expected GET, GET_PROVIDERS, PUT or PUT_PROVIDER");
        }
    }
}
