mod anim;
mod devtools;
mod input;
mod library;
mod theme;
mod ui;

use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use iced::{Color, Size, Subscription, Task, keyboard, time, window};

use anim::{Blend, Spring};
use input::{Action, PadEvent, PadKind};
use library::{Game, Shelf};

fn main() -> iced::Result {
    let fullscreen = std::env::args().any(|a| a == "--fullscreen");
    let mut app = iced::application(App::new, App::update, App::view)
        .title("Superman")
        .subscription(App::subscription)
        .scale_factor(|app: &App| app.scale)
        .default_font(theme::body(iced::font::Weight::Normal))
        .window(window::Settings {
            size: Size::new(1600.0, 900.0),
            min_size: Some(Size::new(960.0, 540.0)),
            fullscreen,
            ..Default::default()
        })
        .antialiasing(true);
    for f in theme::FONT_FILES {
        app = app.font(*f);
    }
    app.run()
}

#[derive(Debug, Clone)]
pub enum Message {
    Pad(PadEvent),
    Key(Action, bool),
    Frame(Instant),
    Tick,
    Resized(Size),
    DevShoot,
    DevShot(window::Screenshot),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Hero buttons: 0 = Play, 1 = Details.
    Hero(usize),
    Shelf,
}

struct Held {
    action: Action,
    next: Instant,
    repeats: u32,
}

pub struct App {
    pub games: Vec<Game>,
    pub shelves: Vec<Shelf>,
    pub focus: Focus,
    /// Current shelf row (kept while focus is on the hero).
    pub row: usize,
    /// Remembered column for every row.
    pub cols: Vec<usize>,
    /// Game shown in the hero.
    pub selected: usize,

    pub scroll_x: Vec<Spring>,
    pub scroll_y: Spring,
    /// Focus growth per cover, 0..1.
    pub grow: Vec<Vec<Blend>>,
    pub hero_focus: Blend,
    ambient_from: (Color, Color),
    ambient_to: (Color, Color),
    ambient_t: Blend,

    pub pad: PadKind,
    held: Option<Held>,
    last_frame: Option<Instant>,
    pub scale: f32,
    pub reduce_motion: bool,
    pub toast: Option<(String, Instant)>,
    pub clock: DateTime<Local>,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        (Self::init(), devtools::boot())
    }

    fn init() -> Self {
        let games = library::fake();
        let shelves = library::build_shelves(&games);
        let selected = shelves.first().and_then(|s| s.games.first()).copied().unwrap_or(0);
        let ambient = games.get(selected).map(|g| (g.ambient, g.accent)).unwrap_or((theme::NIGHT, theme::DUSK));
        let mut grow: Vec<Vec<Blend>> = shelves.iter().map(|s| vec![Blend::new(0.0); s.games.len()]).collect();
        if let Some(first) = grow.first_mut().and_then(|r| r.first_mut()) {
            first.snap(1.0);
        }
        Self {
            focus: Focus::Shelf,
            row: 0,
            cols: vec![0; shelves.len()],
            selected,
            scroll_x: vec![Spring::new(0.0); shelves.len()],
            scroll_y: Spring::new(0.0),
            grow,
            hero_focus: Blend::new(0.0),
            ambient_from: ambient,
            ambient_to: ambient,
            ambient_t: Blend::new(1.0),
            pad: PadKind::Keyboard,
            held: None,
            last_frame: None,
            scale: 900.0 / theme::DESIGN_HEIGHT,
            reduce_motion: std::env::var_os("SUPERMAN_REDUCE_MOTION").is_some(),
            toast: None,
            clock: Local::now(),
            games,
            shelves,
        }
    }

    /// Current blended (ambient, accent) colours of the background.
    pub fn ambient(&self) -> (Color, Color) {
        let t = self.ambient_t.get();
        (
            theme::lerp(self.ambient_from.0, self.ambient_to.0, t),
            theme::lerp(self.ambient_from.1, self.ambient_to.1, t),
        )
    }

    fn animating(&self) -> bool {
        self.held.is_some()
            || self.toast.is_some()
            || !self.scroll_y.settled()
            || !self.ambient_t.settled()
            || !self.hero_focus.settled()
            || self.scroll_x.iter().any(|s| !s.settled())
            || self.grow.iter().flatten().any(|g| !g.settled())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Pad(PadEvent::Connected(kind)) => self.pad = kind,
            Message::Pad(PadEvent::Disconnected) => self.pad = PadKind::Keyboard,
            Message::Pad(PadEvent::Press(a)) => self.press(a),
            Message::Pad(PadEvent::Release(a)) => self.release(a),
            Message::Key(a, true) => {
                self.pad = PadKind::Keyboard;
                self.press(a)
            }
            Message::Key(a, false) => self.release(a),
            Message::Frame(now) => self.frame(now),
            Message::Tick => self.clock = Local::now(),
            Message::DevShoot => return devtools::shoot(),
            Message::DevShot(s) => return devtools::save(s),
            Message::Resized(logical) => {
                let physical_h = logical.height * self.scale;
                self.scale = (physical_h / theme::DESIGN_HEIGHT).max(0.25);
            }
        }
        Task::none()
    }

    fn press(&mut self, a: Action) {
        if a.is_direction() {
            self.held = Some(Held { action: a, next: Instant::now() + Duration::from_millis(400), repeats: 0 });
        }
        self.act(a);
    }

    fn release(&mut self, a: Action) {
        if self.held.as_ref().is_some_and(|h| h.action == a) {
            self.held = None;
        }
    }

    fn frame(&mut self, now: Instant) {
        let dt = self.last_frame.map_or(0.016, |t| (now - t).as_secs_f32());
        self.last_frame = Some(now);
        let instant = self.reduce_motion;

        if let Some(h) = &mut self.held {
            if now >= h.next {
                // Accelerate: 130ms → 55ms.
                let gap = 130u64.saturating_sub(h.repeats as u64 * 12).max(55);
                h.next = now + Duration::from_millis(gap);
                h.repeats += 1;
                let a = h.action;
                self.act(a);
            }
        }
        if self.toast.as_ref().is_some_and(|(_, t)| now > *t) {
            self.toast = None;
        }

        self.scroll_y.step(dt, instant);
        self.hero_focus.step(dt, instant);
        // Colour crossfades even with reduced motion (it's not movement).
        self.ambient_t.step(dt, false);
        for s in &mut self.scroll_x {
            s.step(dt, instant);
        }
        for g in self.grow.iter_mut().flatten() {
            g.step(dt, instant);
        }
        if !self.animating() {
            self.last_frame = None;
        }
    }

    fn act(&mut self, a: Action) {
        let rows = self.shelves.len();
        if rows == 0 {
            return;
        }
        match (self.focus, a) {
            (_, Action::Search) => self.notice("Search arrives in milestone 6."),
            (_, Action::Menu) => self.notice("The quick menu arrives in milestone 8."),
            (Focus::Hero(_), Action::Left) => self.focus = Focus::Hero(0),
            (Focus::Hero(_), Action::Right) => self.focus = Focus::Hero(1),
            (Focus::Hero(_), Action::Down | Action::Back) => self.focus = Focus::Shelf,
            (Focus::Hero(0), Action::Confirm) => self.play(),
            (Focus::Hero(_), Action::Confirm) => self.details(),
            (Focus::Hero(_), _) => {}

            (Focus::Shelf, Action::Up) if self.row == 0 => self.focus = Focus::Hero(0),
            (Focus::Shelf, Action::Up) => self.row -= 1,
            (Focus::Shelf, Action::Down) => self.row = (self.row + 1).min(rows - 1),
            (Focus::Shelf, Action::Left) => self.move_col(-1),
            (Focus::Shelf, Action::Right) => self.move_col(1),
            (Focus::Shelf, Action::PrevTab) => self.move_col(-5),
            (Focus::Shelf, Action::NextTab) => self.move_col(5),
            (Focus::Shelf, Action::Confirm) => self.play(),
            (Focus::Shelf, Action::Details) => self.details(),
            (Focus::Shelf, Action::Back) => {
                if self.row == 0 && self.cols[0] == 0 {
                    self.focus = Focus::Hero(0);
                } else {
                    self.row = 0;
                    self.cols[0] = 0;
                }
            }
        }
        self.sync_focus();
    }

    fn move_col(&mut self, delta: i32) {
        let len = self.shelves[self.row].games.len() as i32;
        let c = &mut self.cols[self.row];
        *c = (*c as i32 + delta).clamp(0, len - 1) as usize;
    }

    /// Point every animation at the new focus.
    fn sync_focus(&mut self) {
        let on_shelf = self.focus == Focus::Shelf;
        self.hero_focus.set(if on_shelf { 0.0 } else { 1.0 });
        self.scroll_y.set(self.row as f32 * ui::home::ROW_H);
        for (r, row) in self.grow.iter_mut().enumerate() {
            let col = self.cols[r];
            self.scroll_x[r].set(col as f32 * (theme::cover::W + theme::cover::GAP));
            for (c, g) in row.iter_mut().enumerate() {
                g.set(if on_shelf && r == self.row && c == col { 1.0 } else { 0.0 });
            }
        }
        let game = self.shelves[self.row].games[self.cols[self.row]];
        if game != self.selected {
            self.selected = game;
            let cur = self.ambient();
            self.ambient_from = cur;
            let g = &self.games[game];
            self.ambient_to = (g.ambient, g.accent);
            self.ambient_t.snap(0.0);
            self.ambient_t.set(1.0);
        }
    }

    fn play(&mut self) {
        let t = self.games[self.selected].title.clone();
        self.notice(&format!("{t} is a placeholder. Launching arrives in milestone 4."));
    }

    fn details(&mut self) {
        self.notice("Game pages arrive in milestone 5.");
    }

    fn notice(&mut self, text: &str) {
        self.toast = Some((text.to_owned(), Instant::now() + Duration::from_millis(2600)));
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            input::gamepads().map(Message::Pad),
            keyboard::listen().filter_map(|e| match e {
                keyboard::Event::KeyPressed { key, repeat: false, .. } => input::map_key(&key).map(|a| Message::Key(a, true)),
                keyboard::Event::KeyReleased { key, .. } => input::map_key(&key).map(|a| Message::Key(a, false)),
                _ => None,
            }),
            window::resize_events().map(|(_, s)| Message::Resized(s)),
            time::every(Duration::from_secs(10)).map(|_| Message::Tick),
        ];
        if self.animating() {
            subs.push(window::frames().map(Message::Frame));
        }
        Subscription::batch(subs)
    }

    fn view(&self) -> iced::Element<'_, Message> {
        ui::home::view(self)
    }
}
