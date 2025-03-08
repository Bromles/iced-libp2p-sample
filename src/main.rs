#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use std::hash::Hash;
use std::sync::Arc;

use crate::handlers::{
    handle_get_record, handle_key_text_changed, handle_p2p_event, handle_put_record,
    handle_value_text_changed,
};
use crate::p2p::{P2pCommand, P2pEvent};
use crate::widgets::{event_log, input_section, network_status};
use iced::advanced::subscription::{EventStream, Hasher, Recipe, from_recipe};
use iced::futures::StreamExt;
use iced::futures::channel::mpsc;
use iced::futures::lock::Mutex;
use iced::futures::stream::BoxStream;
use iced::widget::{self, column};
use iced::window::Position;
use iced::{Element, Fill, Subscription, Task, Theme};
use tracing::{trace, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod handlers;
mod p2p;
mod widgets;

fn main() -> iced::Result {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "p2p_application_development_Bromles=debug,wgpu_core=info".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .expect("Failed to set up logger");

    iced::application("P2P Iced", App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        .position(Position::Centered)
        .run_with(App::new)
}

struct App {
    p2p_control: mpsc::Sender<P2pCommand>,
    p2p_events: Arc<Mutex<mpsc::Receiver<P2pEvent>>>,
    state: State,
}

#[derive(Debug, Clone)]
enum Message {
    P2pEvent(P2pEvent),
    KeyTextChanged(String),
    ValueTextChanged(String),
    PutRecord(String, String),
    GetRecord(String),
    ServerStarted,
    Ignore,
}

#[derive(Debug, Default)]
struct State {
    event_log: Vec<P2pEvent>,
    peer_count: usize,
    current_key: String,
    current_value: String,
}

impl App {
    fn new() -> (Self, Task<Message>) {
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

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::P2pEvent(event) => handle_p2p_event(&mut self.state, event),
            Message::ServerStarted => Task::none(),
            Message::Ignore => Task::none(),
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

    fn subscription(&self) -> Subscription<Message> {
        from_recipe(P2pSub(self.p2p_events.clone()))
    }

    fn theme(&self) -> Theme {
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

    fn view(&self) -> Element<Message> {
        let network_status = network_status(self.state.peer_count);
        let input_section = input_section(&self.state.current_key, &self.state.current_value);
        let event_log = event_log(&self.state.event_log);

        column![network_status, input_section, event_log]
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
