use crate::app::Message;
use crate::p2p::P2pEvent;
use iced::widget::{button, center, column, row, scrollable, text, text_input};
use iced::{Center, Element, Fill, color};

pub fn network_status<'a>(peer_count: usize) -> Element<'a, Message> {
    let connected_peers = text(format!("Connected peers: {peer_count}"));

    row![connected_peers].spacing(10).padding(10).into()
}

pub fn event_log(events: &[P2pEvent]) -> Element<Message> {
    if events.is_empty() {
        center(text("Events will appear here...").color(color!(0x888888))).into()
    } else {
        let events_elements = events
            .iter()
            .map(|m| format!("{m}"))
            .map(text)
            .map(Element::from);

        scrollable(column(events_elements).spacing(10))
            .height(Fill)
            .into()
    }
}

pub fn input_section<'a>(current_key: &str, current_value: &str) -> Element<'a, Message> {
    let key_input = text_input("Key", current_key)
        .on_input(Message::KeyTextChanged)
        .padding(10);

    let value_input = text_input("Value", current_value)
        .on_input(Message::ValueTextChanged)
        .padding(10);

    let mut put_button = button(text("Put").height(40).align_y(Center)).padding([0, 20]);
    let mut get_button = button(text("Get").height(40).align_y(Center)).padding([0, 20]);

    if !current_key.is_empty() && !current_value.is_empty() {
        put_button = put_button.on_press(Message::PutRecord(
            current_key.to_owned(),
            current_value.to_owned(),
        ));
    } else if !current_key.is_empty() && current_value.is_empty() {
        get_button = get_button.on_press(Message::GetRecord(current_key.to_owned()));
    }

    row![key_input, value_input, put_button, get_button]
        .spacing(10)
        .padding(10)
        .into()
}
