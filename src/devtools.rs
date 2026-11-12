//! Dev-only automation: script inputs and capture a screenshot of the app itself.
//!
//! `SUPERMAN_KEYS="r,r,d,a"` presses actions (u/d/l/r, a=confirm, b=back, x, y, m, lb, rb).
//! `SUPERMAN_SHOT=/path.png` saves a screenshot after the keys, then exits.
//! `SUPERMAN_SHOT_DELAY=ms` waits longer before the screenshot (default 900).

use std::time::Duration;

use iced::futures::SinkExt;
use iced::window::{self, Screenshot};
use iced::{Task, stream};

use crate::Message;
use crate::input::Action;

pub fn boot() -> Task<Message> {
    let keys = std::env::var("SUPERMAN_KEYS").unwrap_or_default();
    let shot = std::env::var_os("SUPERMAN_SHOT").is_some();
    if keys.is_empty() && !shot {
        return Task::none();
    }
    let actions: Vec<Action> = keys.split(',').filter_map(|k| parse(k.trim())).collect();
    Task::stream(stream::channel(16, async move |mut out| {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        for a in actions {
            let _ = out.send(Message::Key(a, true)).await;
            let _ = out.send(Message::Key(a, false)).await;
            tokio::time::sleep(Duration::from_millis(220)).await;
        }
        if shot {
            let ms = std::env::var("SUPERMAN_SHOT_DELAY").ok().and_then(|v| v.parse().ok()).unwrap_or(900);
            tokio::time::sleep(Duration::from_millis(ms)).await;
            let _ = out.send(Message::DevShoot).await;
        }
    }))
}

pub fn shoot() -> Task<Message> {
    window::latest().and_then(window::screenshot).map(Message::DevShot)
}

pub fn save(s: Screenshot) -> Task<Message> {
    if let Some(path) = std::env::var_os("SUPERMAN_SHOT") {
        if let Err(e) = image::save_buffer(&path, &s.rgba, s.size.width, s.size.height, image::ColorType::Rgba8) {
            eprintln!("screenshot failed: {e}");
        }
    }
    iced::exit()
}

fn parse(k: &str) -> Option<Action> {
    Some(match k {
        "u" => Action::Up,
        "d" => Action::Down,
        "l" => Action::Left,
        "r" => Action::Right,
        "a" => Action::Confirm,
        "b" => Action::Back,
        "x" => Action::Details,
        "y" => Action::Search,
        "m" => Action::Menu,
        "g" => Action::Guide,
        "lb" => Action::PrevTab,
        "rb" => Action::NextTab,
        _ => return None,
    })
}
