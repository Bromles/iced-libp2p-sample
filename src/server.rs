use libp2p::futures::StreamExt;
use libp2p::kad::store::MemoryStore;
use libp2p::kad::Mode;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{kad, mdns, noise, tcp, yamux, Swarm, SwarmBuilder};
use tokio::io::AsyncBufReadExt;
use tokio::{io, select};

#[derive(NetworkBehaviour)]
struct CustomBehavior {
    kademlia: kad::Behaviour<MemoryStore>,
    mdns: mdns::tokio::Behaviour,
}

pub async fn run() {
    let mut swarm: Swarm<CustomBehavior> = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .expect("")
        .with_quic()
        .with_behaviour(|key| {
            Ok(CustomBehavior {
                kademlia: kad::Behaviour::new(
                    key.public().to_peer_id(),
                    MemoryStore::new(key.public().to_peer_id()),
                ),
                mdns: mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    key.public().to_peer_id(),
                )
                .expect(""),
            })
        })
        .expect("")
        .build();

    swarm.behaviour_mut().kademlia.set_mode(Some(Mode::Server));

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    swarm
        .listen_on("/ip4/0.0.0.0/tcp/0".parse().expect(""))
        .expect("");

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                handle_input_line(&mut swarm.behaviour_mut().kademlia, line)
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr {address, ..} => {
                    println!("Listening in {address:?}");
                }
                SwarmEvent::Behaviour(CustomBehaviorEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, multiaddr) in list {
                        println!("Discovered peer {peer_id} at {multiaddr}");
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);
                    }
                }
                SwarmEvent::Behaviour(CustomBehaviorEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, multiaddr) in list {
                        println!("Expired peer {peer_id} at {multiaddr}");
                        swarm.behaviour_mut().kademlia.remove_address(&peer_id, &multiaddr);
                    }
                }
                SwarmEvent::Behaviour(CustomBehaviorEvent::Kademlia(kad::Event::OutboundQueryProgressed {result, ..})) => {
                    match result {
                        kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {key, providers})) => {
                            for peer in providers {
                                println!(
                                    "Peer {peer} provides key {}",
                                    std::str::from_utf8(key.as_ref()).unwrap()
                                );
                            }
                        }
                        kad::QueryResult::GetProviders(Err(err)) => {
                            eprintln!("Failed to get providers: {err:?}");
                        }
                        kad::QueryResult::GetRecord(Ok(
                            kad::GetRecordOk::FoundRecord(kad::PeerRecord {
                                record: kad::Record { key, value, .. },
                                ..
                            })
                        )) => {
                            println!(
                                "Got record {} : {}",
                                std::str::from_utf8(key.as_ref()).unwrap(),
                                std::str::from_utf8(&value).unwrap(),
                            );
                        }
                        kad::QueryResult::GetRecord(Ok(_)) => {}
                        kad::QueryResult::GetRecord(Err(err)) => {
                            eprintln!("Failed to get record: {err:?}");
                        }
                        kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
                            println!(
                                "Successfully put record {}",
                                std::str::from_utf8(key.as_ref()).unwrap()
                            );
                        }
                        kad::QueryResult::PutRecord(Err(err)) => {
                            eprintln!("Failed to put record: {err:?}");
                        }
                        kad::QueryResult::StartProviding(Ok(kad::AddProviderOk { key })) => {
                            println!(
                                "Successfully put provider record {}",
                                std::str::from_utf8(key.as_ref()).unwrap()
                            );
                        }
                        kad::QueryResult::StartProviding(Err(err)) => {
                            eprintln!("Failed to put provider record: {err:?}");
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

fn handle_input_line(kademlia: &mut kad::Behaviour<MemoryStore>, line: String) {
    let mut args = line.split(' ');

    match args.next() {
        Some("GET") => {
            let key = {
                match args.next() {
                    Some(key) => kad::RecordKey::new(&key),
                    None => {
                        eprintln!("Expected key");
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
                        eprintln!("Expected key");
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
                        eprintln!("Expected key");
                        return;
                    }
                }
            };
            let value = {
                match args.next() {
                    Some(value) => value.as_bytes().to_vec(),
                    None => {
                        eprintln!("Expected value");
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
                        eprintln!("Expected key");
                        return;
                    }
                }
            };

            kademlia
                .start_providing(key)
                .expect("Failed to start providing key");
        }
        _ => {
            eprintln!("expected GET, GET_PROVIDERS, PUT or PUT_PROVIDER");
        }
    }
}
