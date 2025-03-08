use std::fmt;
use std::fmt::Formatter;
use std::time::Duration;
use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, select};
use libp2p::futures::StreamExt;
use libp2p::kad::store::{MemoryStore, RecordStore};
use libp2p::kad::{InboundRequest, Mode, QueryResult, StoreInserts};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm, SwarmBuilder, kad, mdns, noise, tcp, yamux};
use tracing::{debug, error, info};

#[derive(Debug, Clone)]
pub enum P2pCommand {
    GetRecord(String),
    GetProviders(String),
    PutRecord(String, Vec<u8>),
    PutProvider(String),
}

#[derive(Debug, Clone)]
pub enum P2pEvent {
    Bootstrapped(Multiaddr),
    PeerDiscovered(PeerId, Multiaddr),
    PeerExpired(PeerId, Multiaddr),
    Outbound(P2pOutboundEvent),
    Inbound(P2pInboundEvent),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum P2pOutboundEvent {
    RecordFound(kad::RecordKey, Vec<u8>),
    ProvidersFound(kad::RecordKey, Vec<PeerId>),
    RecordPut(kad::RecordKey),
    ProviderPut(kad::RecordKey),
}

#[derive(Debug, Clone)]
pub enum P2pInboundEvent {
    ProviderAdded(kad::RecordKey),
    RecordStored(PeerId, kad::RecordKey, Vec<u8>),
}

impl fmt::Display for P2pEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            P2pEvent::Bootstrapped(address) => write!(f, "Listen on {address}"),
            P2pEvent::PeerDiscovered(peer_id, address) => {
                write!(f, "Discovered peer {peer_id} at {address}")
            }
            P2pEvent::PeerExpired(peer_id, address) => {
                write!(f, "Expired peer {peer_id} at {address}")
            }
            P2pEvent::Error(msg) => write!(f, "Something went wrong: {msg}"),
            P2pEvent::Outbound(event) => match event {
                P2pOutboundEvent::RecordFound(key, value) => write!(
                    f,
                    "Outbound: Found record value for {key:?}: {}",
                    String::from_utf8(value.clone()).unwrap()
                ),
                P2pOutboundEvent::ProvidersFound(key, peer_ids) => {
                    write!(f, "Outbound: Found providers for {key:?}: {peer_ids:?}")
                }
                P2pOutboundEvent::RecordPut(key) => {
                    write!(f, "Outbound: Successfully put record with {key:?}")
                }
                P2pOutboundEvent::ProviderPut(key) => {
                    write!(f, "Outbound: Successfully started providing record with {key:?}")
                }
            },
            P2pEvent::Inbound(event) => match event {
                P2pInboundEvent::ProviderAdded(key) => {
                    write!(f, "Inbound: Received new provider for {key:?}")
                }
                P2pInboundEvent::RecordStored(source_id, key, value) => write!(
                    f,
                    "Inbound: Stored new record from {source_id} with {key:?} and value {}",
                    String::from_utf8(value.clone()).unwrap()
                ),
            },
        }
    }
}

#[derive(NetworkBehaviour)]
struct CustomBehaviour {
    kademlia: kad::Behaviour<MemoryStore>,
    mdns: mdns::tokio::Behaviour,
}

pub async fn run(mut commands: mpsc::Receiver<P2pCommand>, mut events: mpsc::Sender<P2pEvent>) {
    let mut kad_config = kad::Config::default();
    kad_config.set_record_filtering(StoreInserts::FilterBoth);

    let mdns_config = mdns::Config {
        ttl: Duration::from_secs(5),
        query_interval: Duration::from_secs(4),
        ..Default::default()
    };

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
                kademlia: kad::Behaviour::with_config(
                    key.public().to_peer_id(),
                    MemoryStore::new(key.public().to_peer_id()),
                    kad_config,
                ),
                mdns: mdns::tokio::Behaviour::new(
                    mdns_config,
                    key.public().to_peer_id(),
                )
                .expect("Failed to set up mDNS behaviour"),
            })
        })
        .expect("Failed to build Swarm")
        .build();

    swarm.behaviour_mut().kademlia.set_mode(Some(Mode::Server));

    swarm
        .listen_on(
            "/ip4/0.0.0.0/tcp/0"
                .parse()
                .expect("Failed to parse multiaddress"),
        )
        .expect("Failed to start a Swarm");

    loop {
        select! {
            cmd = commands.select_next_some() => handle_command(cmd, &mut swarm).await,
            event = swarm.select_next_some() => handle_swarm_event(event, &mut swarm, &mut events).await,
        }
    }
}

async fn handle_command(cmd: P2pCommand, swarm: &mut Swarm<CustomBehaviour>) {
    match cmd {
        P2pCommand::GetRecord(key) => {
            let key = kad::RecordKey::new(&key);
            swarm.behaviour_mut().kademlia.get_record(key);
        }
        P2pCommand::GetProviders(key) => {
            let key = kad::RecordKey::new(&key);
            swarm.behaviour_mut().kademlia.get_providers(key);
        }
        P2pCommand::PutRecord(key, value) => {
            let key = kad::RecordKey::new(&key);
            let record = kad::Record::new(key, value);

            swarm
                .behaviour_mut()
                .kademlia
                .put_record(record, kad::Quorum::One)
                .expect("Failed to store record");
        }
        P2pCommand::PutProvider(key) => {
            let key = kad::RecordKey::new(&key);
            swarm
                .behaviour_mut()
                .kademlia
                .start_providing(key)
                .expect("Failed to start providing key");
        }
    }
}

async fn handle_swarm_event(
    event: SwarmEvent<CustomBehaviourEvent>,
    swarm: &mut Swarm<CustomBehaviour>,
    sender: &mut mpsc::Sender<P2pEvent>,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            info!("Listening on {address:?}");
            sender
                .send(P2pEvent::Bootstrapped(address))
                .await
                .expect("Failed to send");
        }
        SwarmEvent::Behaviour(CustomBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
            for (peer_id, multiaddr) in list {
                info!("Discovered peer {peer_id} at {multiaddr}");
                swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(&peer_id, multiaddr.clone());
                sender
                    .send(P2pEvent::PeerDiscovered(peer_id, multiaddr))
                    .await
                    .expect("Failed to send");
            }
        }
        SwarmEvent::Behaviour(CustomBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
            for (peer_id, multiaddr) in list {
                info!("Expired peer {peer_id} at {multiaddr}");
                swarm
                    .behaviour_mut()
                    .kademlia
                    .remove_address(&peer_id, &multiaddr);
                sender
                    .send(P2pEvent::PeerExpired(peer_id, multiaddr))
                    .await
                    .expect("Failed to send");
            }
        }
        SwarmEvent::Behaviour(CustomBehaviourEvent::Kademlia(
            kad::Event::OutboundQueryProgressed { result, .. },
        )) => handle_outbound_query(result, sender).await,
        SwarmEvent::Behaviour(CustomBehaviourEvent::Kademlia(kad::Event::InboundRequest {
            request,
            ..
        })) => handle_inbound_request(request, swarm, sender).await,
        _ => {}
    }
}

async fn handle_outbound_query(result: QueryResult, sender: &mut mpsc::Sender<P2pEvent>) {
    match result {
        QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { key, providers })) => {
            for peer in &providers {
                info!(
                    "Peer {peer} provides key {}",
                    std::str::from_utf8(key.as_ref()).unwrap()
                );
            }

            sender
                .send(P2pEvent::Outbound(P2pOutboundEvent::ProvidersFound(
                    key,
                    providers.into_iter().collect(),
                )))
                .await
                .expect("Failed to send");
        }
        QueryResult::GetProviders(Err(err)) => {
            error!("Failed to get providers: {err:?}");
            sender
                .send(P2pEvent::Error(format!("{:?}", err)))
                .await
                .expect("Failed to send");
        }
        QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(kad::PeerRecord {
            record: kad::Record { key, value, .. },
            ..
        }))) => {
            info!(
                "Got record {} : {}",
                std::str::from_utf8(key.as_ref()).unwrap(),
                std::str::from_utf8(&value).unwrap(),
            );

            sender
                .send(P2pEvent::Outbound(P2pOutboundEvent::RecordFound(
                    key, value,
                )))
                .await
                .expect("Failed to send");
        }
        QueryResult::GetRecord(Ok(kad::GetRecordOk::FinishedWithNoAdditionalRecord { .. })) => {
            debug!("GetRecord outbound query finished with no additional record");
        }
        QueryResult::GetRecord(Err(err)) => {
            error!("Failed to get record: {err:?}");
            sender
                .send(P2pEvent::Error(format!("{:?}", err)))
                .await
                .expect("Failed to send");
        }
        QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
            info!(
                "Successfully put record {}",
                std::str::from_utf8(key.as_ref()).unwrap()
            );

            sender
                .send(P2pEvent::Outbound(P2pOutboundEvent::RecordPut(key)))
                .await
                .expect("Failed to send");
        }
        QueryResult::PutRecord(Err(err)) => {
            info!("Failed to put record: {err:?}");
            sender
                .send(P2pEvent::Error(format!("{:?}", err)))
                .await
                .expect("Failed to send");
        }
        QueryResult::StartProviding(Ok(kad::AddProviderOk { key })) => {
            info!(
                "Successfully put provider record {}",
                std::str::from_utf8(key.as_ref()).unwrap()
            );

            sender
                .send(P2pEvent::Outbound(P2pOutboundEvent::ProviderPut(key)))
                .await
                .expect("Failed to send");
        }
        QueryResult::StartProviding(Err(err)) => {
            error!("Failed to put provider record: {err:?}");
            sender
                .send(P2pEvent::Error(format!("{:?}", err)))
                .await
                .expect("Failed to send");
        }
        _ => {}
    }
}

async fn handle_inbound_request(
    request: InboundRequest,
    swarm: &mut Swarm<CustomBehaviour>,
    sender: &mut mpsc::Sender<P2pEvent>,
) {
    info!("Inbound request: {request:?}");

    match request {
        InboundRequest::AddProvider {
            record: Some(record),
        } => {
            let store = swarm.behaviour_mut().kademlia.store_mut();
            store
                .add_provider(record.clone())
                .expect("Failed to store provider record");
            sender
                .send(P2pEvent::Inbound(P2pInboundEvent::ProviderAdded(
                    record.key,
                )))
                .await
                .expect("Failed to send");
        }
        InboundRequest::PutRecord {
            source,
            record: Some(record),
            ..
        } => {
            let store = swarm.behaviour_mut().kademlia.store_mut();
            store.put(record.clone()).expect("Failed to store record");
            sender
                .send(P2pEvent::Inbound(P2pInboundEvent::RecordStored(
                    source,
                    record.key,
                    record.value,
                )))
                .await
                .expect("Failed to send");
        }
        _ => {}
    }
}
