use std::hash::Hash;
use std::sync::{Arc, LazyLock};

use iced::{Center, color, Element, Fill, Subscription, Task, Theme};
use iced::advanced::subscription::{EventStream, from_recipe, Hasher, Recipe};
use iced::futures::{SinkExt, StreamExt};
use iced::futures::channel::mpsc;
use iced::futures::lock::Mutex;
use iced::futures::stream::BoxStream;
use iced::widget::{self, button, center, column, row, scrollable, text, text_input};
use iced::window::Position;
use tracing::{info, Level, warn};

use crate::p2p::{P2pCommand, P2pEvent};

mod p2p;

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
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
}

#[derive(Debug, Clone)]
enum Message {
    P2pEvent(P2pEvent),
    UserInput(String),
    ServerStarted,
    Ignore,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let (command_sender, command_receiver) = mpsc::channel(100);
        let (event_sender, event_receiver) = mpsc::channel(100);

        (
            Self {
                p2p_control: command_sender,
                p2p_events: Arc::new(Mutex::new(event_receiver)),
            },
            Task::batch([
                Task::perform(p2p::run(command_receiver, event_sender), |_| Message::ServerStarted),
                widget::focus_next(),
            ]),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::P2pEvent(event) => handle_p2p_event(event),
            Message::UserInput(data) => handle_user_input(data, self.p2p_control.clone()),
            Message::ServerStarted => Task::none(),
            Message::Ignore => Task::none(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        from_recipe(SwarmSub(self.p2p_events.clone()))
    }

    fn theme(&self) -> Theme {
        match dark_light::detect().expect("Failed to detect system theme") {
            dark_light::Mode::Light => {
                info!("Detected light system theme");
                Theme::Light
            }
            dark_light::Mode::Dark => {
                info!("Detected dark system theme");
                Theme::Dark
            }
            dark_light::Mode::Unspecified => {
                warn!("System theme is not specified");
                Theme::Dark
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let message_log: Element<_> = if self.messages.is_empty() {
            center(text("Your messages will appear here...").color(color!(0x888888))).into()
        } else {
            scrollable(column(self.messages.iter().map(text).map(Element::from)).spacing(10))
                .id(MESSAGE_LOG.clone())
                .height(Fill)
                .into()
        };

        let new_message_input = {
            let mut input = text_input("Type a message...", &self.new_message)
                .on_input(Message::NewMessageChanged)
                .padding(10);

            let mut button = button(text("Send").height(40).align_y(Center)).padding([0, 20]);

            if matches!(self.state, State::Connected(_)) {
                // if let Some(message) = echo::Message::new(&self.new_message) {
                //     input = input.on_submit(Message::Send(message.clone()));
                //     button = button.on_press(Message::Send(message));
                // }
            }

            row![input, button].spacing(10).align_y(Center)
        };

        column![message_log, new_message_input]
            .height(Fill)
            .padding(20)
            .spacing(10)
            .into()
    }
}

struct SwarmSub(Arc<Mutex<mpsc::Receiver<P2pEvent>>>);

impl Recipe for SwarmSub {
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

fn handle_p2p_event(event: P2pEvent) -> Task<Message> {
    Task::none()
} 

fn handle_user_input(data: String, mut sender: mpsc::Sender<P2pCommand>) -> Task<Message> {
    let cmd = P2pCommand::PutRecord(data.clone(), data.into_bytes());
    
    Task::perform(async move {
        sender.send(cmd).await.ok()
    }, |_| Message::Ignore)
}
