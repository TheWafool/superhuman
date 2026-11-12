//! Turns gamepads and keyboard into semantic actions.

use std::time::Duration;

use gilrs::{Axis, Button, EventType, Gilrs};
use iced::futures::SinkExt;
use iced::keyboard::{Key, key::Named};
use iced::{Subscription, stream};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    /// A / Cross
    Confirm,
    /// B / Circle
    Back,
    /// X / Square
    Details,
    /// Y / Triangle
    Search,
    /// Start / Menu
    Menu,
    PrevTab,
    NextTab,
    /// Guide / PS / Home button: the system menu, even while a game is running.
    Guide,
}

impl Action {
    pub fn is_direction(self) -> bool {
        matches!(self, Action::Up | Action::Down | Action::Left | Action::Right)
    }
}

/// Which button glyphs to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PadKind {
    #[default]
    Keyboard,
    Xbox,
    PlayStation,
    Nintendo,
    Deck,
}

impl PadKind {
    fn from_name(name: &str) -> Self {
        let n = name.to_lowercase();
        if n.contains("playstation") || n.contains("dualsense") || n.contains("dualshock") || n.contains("ps4") || n.contains("ps5") || n.contains("wireless controller") {
            PadKind::PlayStation
        } else if n.contains("nintendo") || n.contains("pro controller") || n.contains("joy-con") || n.contains("switch") {
            PadKind::Nintendo
        } else if n.contains("steam deck") || n.contains("steam controller") {
            PadKind::Deck
        } else {
            PadKind::Xbox
        }
    }

    /// Label shown inside the glyph for an action.
    pub fn glyph(self, action: Action) -> &'static str {
        use Action::*;
        match (self, action) {
            (PadKind::Keyboard, Confirm) => "↵",
            (PadKind::Keyboard, Back) => "Esc",
            (PadKind::Keyboard, Details) => "I",
            (PadKind::Keyboard, Search) => "/",
            (PadKind::Keyboard, Menu) => "M",
            (PadKind::Keyboard, Guide) => "F1",
            (PadKind::PlayStation, Guide) => "PS",
            (PadKind::PlayStation, Confirm) => "✕",
            (PadKind::PlayStation, Back) => "○",
            (PadKind::PlayStation, Details) => "□",
            (PadKind::PlayStation, Search) => "△",
            // Nintendo layout swaps positions; glyph shows the physical label.
            (PadKind::Nintendo, Confirm) => "A",
            (PadKind::Nintendo, Back) => "B",
            (PadKind::Nintendo, Details) => "Y",
            (PadKind::Nintendo, Search) => "X",
            (_, Confirm) => "A",
            (_, Back) => "B",
            (_, Details) => "X",
            (_, Search) => "Y",
            (_, Menu) => "☰",
            (_, PrevTab) => "LB",
            (_, NextTab) => "RB",
            (_, Guide) => "Home",
            _ => "",
        }
    }
}

#[derive(Debug, Clone)]
pub enum PadEvent {
    Press(Action),
    Release(Action),
    Connected(PadKind),
    Disconnected,
}

fn map_button(b: Button) -> Option<Action> {
    Some(match b {
        Button::DPadUp => Action::Up,
        Button::DPadDown => Action::Down,
        Button::DPadLeft => Action::Left,
        Button::DPadRight => Action::Right,
        // gilrs reports positions: South is A on Xbox, Cross on PS, B on Nintendo.
        Button::South => Action::Confirm,
        Button::East => Action::Back,
        Button::West => Action::Details,
        Button::North => Action::Search,
        Button::Start => Action::Menu,
        Button::Mode => Action::Guide,
        Button::LeftTrigger => Action::PrevTab,
        Button::RightTrigger => Action::NextTab,
        _ => return None,
    })
}

const STICK_ON: f32 = 0.55;
const STICK_OFF: f32 = 0.35;

/// Gamepad events from a dedicated thread (gilrs is not `Send`).
pub fn gamepads() -> Subscription<PadEvent> {
    Subscription::run(|| {
        stream::channel(64, async |mut out| {
            let (tx, mut rx) = iced::futures::channel::mpsc::unbounded();
            std::thread::Builder::new()
                .name("gamepad".into())
                .spawn(move || pad_thread(tx))
                .expect("spawn gamepad thread");
            use iced::futures::StreamExt;
            while let Some(ev) = rx.next().await {
                if out.send(ev).await.is_err() {
                    break;
                }
            }
        })
    })
}

fn pad_thread(tx: iced::futures::channel::mpsc::UnboundedSender<PadEvent>) {
    let mut gilrs = match Gilrs::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("gamepad input unavailable: {e}");
            return;
        }
    };
    let send = |ev| tx.unbounded_send(ev).is_ok();

    if let Some((_, pad)) = gilrs.gamepads().next() {
        send(PadEvent::Connected(PadKind::from_name(pad.name())));
    }

    // Digital state of each stick direction, for hysteresis.
    let mut stick: [Option<Action>; 2] = [None, None]; // [x, y]

    loop {
        let Some(ev) = gilrs.next_event_blocking(Some(Duration::from_millis(500))) else {
            if tx.is_closed() {
                return;
            }
            continue;
        };
        let ok = match ev.event {
            EventType::ButtonPressed(b, _) => map_button(b).is_none_or(|a| send(PadEvent::Press(a))),
            EventType::ButtonReleased(b, _) => map_button(b).is_none_or(|a| send(PadEvent::Release(a))),
            EventType::AxisChanged(axis, v, _) => {
                let (slot, neg, pos, v) = match axis {
                    Axis::LeftStickX => (0, Action::Left, Action::Right, v),
                    // gilrs: up is positive.
                    Axis::LeftStickY => (1, Action::Down, Action::Up, v),
                    _ => continue,
                };
                let want = if v > STICK_ON {
                    Some(pos)
                } else if v < -STICK_ON {
                    Some(neg)
                } else if v.abs() < STICK_OFF {
                    None
                } else {
                    stick[slot]
                };
                let mut ok = true;
                if want != stick[slot] {
                    if let Some(old) = stick[slot] {
                        ok &= send(PadEvent::Release(old));
                    }
                    if let Some(new) = want {
                        ok &= send(PadEvent::Press(new));
                    }
                    stick[slot] = want;
                }
                ok
            }
            EventType::Connected => {
                let name = gilrs.gamepad(ev.id).name().to_owned();
                send(PadEvent::Connected(PadKind::from_name(&name)))
            }
            EventType::Disconnected => send(PadEvent::Disconnected),
            _ => true,
        };
        if !ok {
            return;
        }
    }
}

pub fn map_key(key: &Key) -> Option<Action> {
    Some(match key {
        Key::Named(Named::ArrowUp) => Action::Up,
        Key::Named(Named::ArrowDown) => Action::Down,
        Key::Named(Named::ArrowLeft) => Action::Left,
        Key::Named(Named::ArrowRight) => Action::Right,
        Key::Named(Named::Enter | Named::Space) => Action::Confirm,
        Key::Named(Named::Escape | Named::Backspace) => Action::Back,
        Key::Named(Named::PageUp) => Action::PrevTab,
        Key::Named(Named::PageDown) => Action::NextTab,
        Key::Named(Named::F1 | Named::Home) => Action::Guide,
        Key::Character(c) => match c.as_str() {
            "i" => Action::Details,
            "/" => Action::Search,
            "m" => Action::Menu,
            "q" => Action::PrevTab,
            "e" => Action::NextTab,
            _ => return None,
        },
        _ => return None,
    })
}
