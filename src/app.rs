use std::hash::Hash;
use std::sync::Arc;
use iced::futures::channel::mpsc;
use iced::futures::lock::Mutex;
use iced::{keyboard, widget, Element, Fill, Subscription, Task, Theme};
use iced::advanced::subscription::{from_recipe, EventStream, Hasher, Recipe};
use iced::futures::stream::BoxStream;
use iced::futures::StreamExt;
use iced::keyboard::key;
use tracing::{trace, warn};
use crate::handlers::{handle_get_record, handle_key_text_changed, handle_p2p_event, handle_put_record, handle_value_text_changed};
use crate::p2p;
use crate::p2p::{P2pCommand, P2pEvent};
use crate::widgets::{event_log, input_section, network_status};

pub struct App {
    p2p_control: mpsc::Sender<P2pCommand>,
    p2p_events: Arc<Mutex<mpsc::Receiver<P2pEvent>>>,
    state: State,
}

#[derive(Debug, Clone)]
pub enum Message {
    P2pEvent(P2pEvent),
    KeyTextChanged(String),
    ValueTextChanged(String),
    PutRecord(String, String),
    GetRecord(String),
    FocusNext,
    ServerStarted,
    Ignore,
}

#[derive(Debug, Default)]
pub struct State {
    pub event_log: Vec<P2pEvent>,
    pub peer_count: usize,
    pub current_key: String,
    pub current_value: String,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let (command_sender, command_receiver) = mpsc::channel(100);
        let (event_sender, event_receiver) = mpsc::channel(100);

        (
            Self {
                p2p_control: command_sender,
                p2p_events: Arc::new(Mutex::new(event_receiver)),
                state: State::default(),
            },
            Task::batch([
                Task::perform(p2p::run(command_receiver, event_sender), |_| {
                    Message::ServerStarted
                }),
                widget::focus_next(),
            ]),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::P2pEvent(event) => handle_p2p_event(&mut self.state, event),
            Message::ServerStarted => Task::none(),
            Message::Ignore => Task::none(),
            Message::FocusNext => widget::focus_next(),
            Message::KeyTextChanged(data) => handle_key_text_changed(&mut self.state, data),
            Message::ValueTextChanged(data) => handle_value_text_changed(&mut self.state, data),
            Message::PutRecord(key, value) => {
                handle_put_record(&mut self.state, key, value, self.p2p_control.clone())
            }
            Message::GetRecord(key) => {
                handle_get_record(&mut self.state, key, self.p2p_control.clone())
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let p2p_sub = from_recipe(P2pSub(self.p2p_events.clone()));

        let focus_sub = keyboard::on_key_release(|key, _modifiers| match key {
            keyboard::Key::Named(key::Named::Tab) => Some(Message::FocusNext),
            _ => None,
        });

        Subscription::batch([p2p_sub, focus_sub])
    }

    pub fn theme(&self) -> Theme {
        match dark_light::detect().expect("Failed to detect system theme") {
            dark_light::Mode::Light => {
                trace!("Detected light system theme");
                Theme::Light
            }
            dark_light::Mode::Dark => {
                trace!("Detected dark system theme");
                Theme::Dark
            }
            dark_light::Mode::Unspecified => {
                warn!("System theme is not specified");
                Theme::Dark
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let network_status = network_status(self.state.peer_count);
        let input_section = input_section(&self.state.current_key, &self.state.current_value);
        let event_log = event_log(&self.state.event_log);

        iced::widget::column![network_status, input_section, event_log]
            .height(Fill)
            .padding(20)
            .spacing(10)
            .into()
    }
}

struct P2pSub(Arc<Mutex<mpsc::Receiver<P2pEvent>>>);

impl Recipe for P2pSub {
    type Output = Message;

    fn hash(&self, state: &mut Hasher) {
        std::any::TypeId::of::<Self>().hash(state)
    }

    fn stream(self: Box<Self>, _: EventStream) -> BoxStream<'static, Self::Output> {
        Box::pin(async_stream::stream! {
            let mut receiver = self.0.lock().await;

            while let Some(event) = receiver.next().await {
                yield Message::P2pEvent(event)
            }
        })
    }
}
