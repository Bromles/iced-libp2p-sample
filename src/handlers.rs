use crate::p2p::{P2pCommand, P2pEvent};
use iced::Task;
use iced::futures::SinkExt;
use iced::futures::channel::mpsc;
use crate::app::{Message, State};

pub fn handle_p2p_event(state: &mut State, event: P2pEvent) -> Task<Message> {
    state.event_log.push(event.clone());
    
    if let P2pEvent::PeerDiscovered(..) = event {
        state.peer_count += 1;
    } else if let P2pEvent::PeerExpired(..) = event {
        state.peer_count -= 1;
    }

    Task::none()
}

pub fn handle_key_text_changed(state: &mut State, data: String) -> Task<Message> {
    state.current_key = data;

    Task::none()
}

pub fn handle_value_text_changed(state: &mut State, data: String) -> Task<Message> {
    state.current_value = data;

    Task::none()
}

pub fn handle_put_record(
    state: &mut State,
    key: String,
    value: String,
    mut sender: mpsc::Sender<P2pCommand>,
) -> Task<Message> {
    state.current_value = "".to_owned();

    let cmd = P2pCommand::PutRecord(key, value.into_bytes());

    Task::perform(async move { sender.send(cmd).await.ok() }, |_| {
        Message::Ignore
    })
}

pub fn handle_get_record(
    _: &mut State,
    key: String,
    mut sender: mpsc::Sender<P2pCommand>,
) -> Task<Message> {
    let cmd = P2pCommand::GetRecord(key);

    Task::perform(async move { sender.send(cmd).await.ok() }, |_| {
        Message::Ignore
    })
}
