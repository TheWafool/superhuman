mod anim;
mod devtools;
mod input;
mod launch;
mod library;
mod sources;
mod store;
mod theme;
mod ui;

use std::sync::Arc;
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
    Rescan,
    Scanned(Arc<Vec<sources::ScanResult>>),
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
    store: Option<store::Store>,
    scanning: bool,
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
        let mut app = Self::init();
        let scan = app.rescan();
        (app, Task::batch([scan, devtools::boot()]))
    }

    fn init() -> Self {
        // Cached library first, so the first frame already shows games.
        let (store, games) = if std::env::var_os("SUPERMAN_DEMO").is_some() {
            (None, library::demo())
        } else {
            match store::Store::open(&store::default_path()) {
                Ok(s) => {
                    let games = library::from_rows(s.load().unwrap_or_default());
                    (Some(s), games)
                }
                Err(e) => {
                    eprintln!("library database unavailable: {e}");
                    (None, Vec::new())
                }
            }
        };
        let mut app = Self {
            store,
            scanning: false,
            focus: Focus::Shelf,
            row: 0,
            cols: Vec::new(),
            selected: 0,
            scroll_x: Vec::new(),
            scroll_y: Spring::new(0.0),
            grow: Vec::new(),
            hero_focus: Blend::new(0.0),
            ambient_from: (theme::NIGHT, theme::DUSK),
            ambient_to: (theme::NIGHT, theme::DUSK),
            ambient_t: Blend::new(1.0),
            pad: PadKind::Keyboard,
            held: None,
            last_frame: None,
            scale: 900.0 / theme::DESIGN_HEIGHT,
            reduce_motion: std::env::var_os("SUPERMAN_REDUCE_MOTION").is_some(),
            toast: None,
            clock: Local::now(),
            games: Vec::new(),
            shelves: Vec::new(),
        };
        app.set_library(games);
        app.snap_animations();
        app
    }

    /// Replace the library, keeping focus on the same game if it still exists.
    fn set_library(&mut self, games: Vec<Game>) {
        let keep = self.games.get(self.selected).map(|g| g.id.clone());
        let keep_row = self.shelves.get(self.row).map(|s| s.title);
        self.games = games;
        self.shelves = library::build_shelves(&self.games);
        let n = self.shelves.len();
        self.cols = vec![0; n];
        self.scroll_x = vec![Spring::new(0.0); n];
        self.grow = self.shelves.iter().map(|s| vec![Blend::new(0.0); s.games.len()]).collect();
        self.row = keep_row.and_then(|t| self.shelves.iter().position(|s| s.title == t)).unwrap_or(0).min(n.saturating_sub(1));
        if let Some(id) = keep {
            if let Some(gi) = self.games.iter().position(|g| g.id == id) {
                let in_row = |r: usize| self.shelves[r].games.iter().position(|&g| g == gi);
                if let Some(c) = self.shelves.get(self.row).and_then(|_| in_row(self.row)) {
                    self.cols[self.row] = c;
                } else if let Some((r, c)) = (0..n).find_map(|r| in_row(r).map(|c| (r, c))) {
                    self.row = r;
                    self.cols[r] = c;
                }
            }
        }
        for r in 0..n {
            self.scroll_x[r].snap(self.cols[r] as f32 * (theme::cover::W + theme::cover::GAP));
        }
        self.selected = usize::MAX; // force sync_focus to refresh the hero
        self.sync_focus();
        for (r, row) in self.grow.iter_mut().enumerate() {
            for (c, g) in row.iter_mut().enumerate() {
                g.snap(if self.focus == Focus::Shelf && r == self.row && c == self.cols[r] { 1.0 } else { 0.0 });
            }
        }
    }

    pub fn scanning(&self) -> bool {
        self.scanning
    }

    fn snap_animations(&mut self) {
        self.ambient_from = self.ambient_to;
        self.ambient_t.snap(1.0);
        self.scroll_y.snap(self.scroll_y.target);
    }

    fn rescan(&mut self) -> Task<Message> {
        if self.store.is_none() || self.scanning {
            return Task::none();
        }
        self.scanning = true;
        Task::perform(
            async { tokio::task::spawn_blocking(sources::scan_all).await.unwrap_or_default() },
            |r| Message::Scanned(Arc::new(r)),
        )
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
            Message::Rescan => return self.rescan(),
            Message::Scanned(results) => {
                self.scanning = false;
                for r in results.iter() {
                    if let Err(e) = &r.games {
                        eprintln!("{} scan failed: {e}", r.source.label());
                    }
                }
                if let Some(store) = &mut self.store {
                    match store.apply(&results) {
                        Ok(true) => {
                            let games = library::from_rows(store.load().unwrap_or_default());
                            self.set_library(games);
                        }
                        Ok(false) => {}
                        Err(e) => eprintln!("saving library failed: {e}"),
                    }
                }
            }
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
        let Some(&game) = self.shelves.get(self.row).and_then(|s| s.games.get(self.cols[self.row])) else { return };
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
        let Some(g) = self.games.get(self.selected) else { return };
        let title = g.title.clone();
        match launch::start(&g.launch) {
            Ok(()) => {
                let now = Local::now();
                if let Some(store) = &self.store {
                    let _ = store.mark_played(&g.id, now.timestamp());
                }
                self.games[self.selected].last_played = Some(now);
                self.notice(&format!("Starting {title}"));
            }
            Err(e) => self.notice(&format!("{title} didn't start: {e}")),
        }
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
            // Cheap: scanners only read a few local files.
            time::every(Duration::from_secs(45)).map(|_| Message::Rescan),
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
