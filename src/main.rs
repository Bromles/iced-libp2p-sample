use std::sync::{Arc, LazyLock};

use iced::{Center, color, Element, Fill, Subscription, Task, Theme};
use iced::advanced::graphics::futures::{boxed_stream, BoxStream};
use iced::advanced::subscription;
use iced::advanced::subscription::{EventStream, Hasher};
use iced::futures::channel::mpsc;
use iced::futures::lock::Mutex;
use iced::futures::StreamExt;
use iced::widget::{self, button, center, column, row, scrollable, text, text_input};
use iced::window::Position;
use tracing::{info, Level, warn};

mod p2p;

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .try_init()
        .expect("Failed to set up logger");

    iced::application("P2P Iced", P2pApp::update, P2pApp::view)
        .subscription(P2pApp::subscription)
        .theme(P2pApp::theme)
        .position(Position::Centered)
        .run_with(P2pApp::new)
}

static MESSAGE_LOG: LazyLock<scrollable::Id> = LazyLock::new(scrollable::Id::unique);

struct P2pApp {
    swarm_control: mpsc::Sender<SwarmCommand>,
    swarm_events: mpsc::Receiver<SwarmEvent>,
}

#[derive(Debug, Clone)]
enum SwarmCommand {
    Bootstrap,
    GetRecord(String),
    GetProviders(String),
    PutRecord(String, Vec<u8>),
    PutProvider(String),
}

#[derive(Debug, Clone)]
enum SwarmEvent {
    Bootstrapped,
    RecordFound(Vec<u8>),
    Error(String)
}

#[derive(Debug, Clone)]
enum Message {
    SwarmEvent(SwarmEvent),
    UserInput(String),
    ServerStarted
}

impl P2pApp {
    fn new() -> (Self, Task<Message>) {
        let (command_sender, command_receiver) = mpsc::channel(100);
        let (event_sender, event_receiver) = mpsc::channel(100);
        
        (
            Self {
                swarm_control: command_sender,
                swarm_events: event_receiver
            },
            Task::batch([
                Task::perform(p2p::run(command_receiver, event_sender), |_| Message::ServerStarted),
                widget::focus_next(),
            ]),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NewMessageChanged(new_message) => {
                self.new_message = new_message;

                Task::none()
            }
            Message::Send(message) => match &mut self.state {
                State::Connected(connection) => {
                    self.new_message.clear();

                    info!("Connected {connection} with message {message}");

                    Task::none()
                }
                State::Disconnected => Task::none(),
            },
            Message::Echo(event) => {
                info!("Echo {event}");

                Task::none()
            }
            /*Message::Echo(event) => match event {
                echo::Event::Connected(connection) => {
                    self.state = State::Connected(connection);
                
                    self.messages.push(echo::Message::connected());
                
                    Task::none()
                }
                echo::Event::Disconnected => {
                    self.state = State::Disconnected;
                
                    self.messages.push(echo::Message::disconnected());
                
                    Task::none()
                }
                echo::Event::MessageReceived(message) => {
                    self.messages.push(message);
                
                    scrollable::snap_to(MESSAGE_LOG.clone(), scrollable::RelativeOffset::END)
                }
            },*/
            Message::Server => Task::none(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        struct SwarmSub(Arc<Mutex<mpsc::Receiver<SwarmEvent>>>);
        
        impl subscription::Recipe for SwarmSub {
            type Output = Message;

            fn hash(&self, state: &mut Hasher) {
                todo!()
            }

            fn stream(self: Box<Self>, input: EventStream) -> BoxStream<Self::Output> {
                let receiver = self.0.clone();
                
                let mut recv = receiver.lock().await;
                
                while let Some(event) = recv.next().await {
                    Message::SwarmEvent(event);
                }
            }
        }
        
        Subscription::from_recipe(SwarmSub(self.swarm_events.close()));
        
        Subscription::none()
        //Subscription::run(echo::connect).map(Message::Echo)
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
