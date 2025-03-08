#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use std::hash::Hash;
use std::sync::{Arc, LazyLock};

use iced::advanced::subscription::{EventStream, Hasher, Recipe, from_recipe};
use iced::futures::channel::mpsc;
use iced::futures::lock::Mutex;
use iced::futures::stream::BoxStream;
use iced::futures::{SinkExt, StreamExt};
use iced::widget::{self, button, center, column, row, scrollable, text, text_input};
use iced::window::Position;
use iced::{Center, Element, Fill, Subscription, Task, Theme, color};
use tracing::{trace, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::p2p::{P2pCommand, P2pEvent};

mod p2p;

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

static MESSAGE_LOG: LazyLock<scrollable::Id> = LazyLock::new(scrollable::Id::unique);

struct App {
    p2p_control: mpsc::Sender<P2pCommand>,
    p2p_events: Arc<Mutex<mpsc::Receiver<P2pEvent>>>,
    state: State,
}

#[derive(Debug, Clone)]
enum Message {
    P2pEvent(P2pEvent),
    CurrentMessageChanged(String),
    UserInput(String),
    ServerStarted,
    Ignore,
}

#[derive(Debug, Default)]
struct State {
    messages: Vec<P2pEvent>,
    current_message: String,
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
            Message::CurrentMessageChanged(data) => handle_new_message(&mut self.state, data),
            Message::UserInput(data) => {
                handle_user_input(&mut self.state, data, self.p2p_control.clone())
            }
            Message::ServerStarted => Task::none(),
            Message::Ignore => Task::none(),
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
        let message_log: Element<_> = message_log(&self.state.messages);
        let new_message_input = new_message_input(&self.state.current_message);

        column![message_log, new_message_input]
            .height(Fill)
            .padding(20)
            .spacing(10)
            .into()
    }
}

fn message_log(messages: &[P2pEvent]) -> Element<'_, Message> {
    if messages.is_empty() {
        center(text("Your messages will appear here...").color(color!(0x888888))).into()
    } else {
        let messages_elements = messages
            .iter()
            .map(|m| format!("{m}"))
            .map(text)
            .map(Element::from);

        scrollable(column(messages_elements).spacing(10))
            .id(MESSAGE_LOG.clone())
            .height(Fill)
            .into()
    }
}

fn new_message_input(current_message: &str) -> Element<'_, Message> {
    let mut input = text_input("Type a message...", current_message)
        .on_input(Message::CurrentMessageChanged)
        .padding(10);

    let mut button = button(text("Send").height(40).align_y(Center)).padding([0, 20]);

    if !current_message.is_empty() {
        input = input.on_submit(Message::UserInput(current_message.to_owned()));
        button = button.on_press(Message::UserInput(current_message.to_owned()));
    }

    row![input, button].spacing(10).align_y(Center).into()
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

fn handle_p2p_event(state: &mut State, event: P2pEvent) -> Task<Message> {
    state.messages.push(event);

    Task::none()
}

fn handle_new_message(state: &mut State, data: String) -> Task<Message> {
    state.current_message = data.clone();

    Task::none()
}

fn handle_user_input(
    state: &mut State,
    data: String,
    mut sender: mpsc::Sender<P2pCommand>,
) -> Task<Message> {
    state.current_message = "".to_owned();

    let cmd = P2pCommand::PutRecord(data.clone(), data.into_bytes());

    Task::perform(async move { sender.send(cmd).await.ok() }, |_| {
        Message::Ignore
    })
}
