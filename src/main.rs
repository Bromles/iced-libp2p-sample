#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use crate::app::App;
use iced::window::Position;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod handlers;
mod p2p;
mod widgets;
mod app;

fn main() -> iced::Result {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "iced-libp2p-sample=debug,wgpu_core=info".into()
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

