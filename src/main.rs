mod anim;
mod art;
mod config;
mod devtools;
mod input;
mod launch;
mod library;
mod meta;
mod osk;
mod sources;
mod store;
mod theme;
mod ui;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
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
    /// Raw keyboard input, for typing.
    RawKey(keyboard::Key, Option<String>, bool),
    Frame(Instant),
    Tick,
    Resized(Size),
    Rescan,
    Scanned(Arc<Vec<sources::ScanResult>>),
    ArtFetched(String, art::Art, bool),
    Allocated(PathBuf, Result<iced::widget::image::Allocation, iced::widget::image::Error>),
    /// Check on the running game.
    Poll,
    MetaFetched(String, meta::Meta),
    Candidates(String, Vec<art::Candidate>),
    Choices(String, art::Slot, Vec<art::Choice>),
    Thumb(String, Option<PathBuf>),
    DevShoot,
    DevShot(window::Screenshot),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    /// Game page for `selected`.
    Game,
    /// "Which game is this?" list.
    Match,
    /// Art picker.
    Art,
    /// Search with filters and the on-screen keyboard.
    Search,
    /// Text entry for something else (see `typing_for`).
    Typing,
    Settings,
    /// Hidden games, to unhide.
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypingFor {
    MatchName,
    ApiKey,
}

/// Settings rows, in order. Labels and values come from `App::settings_rows`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingRow {
    ApiKey,
    ReduceMotion,
    Rescan,
    Hidden,
}

/// Game page buttons, in order. Labels come from `App::page_buttons`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageButton {
    Play,
    Favourite,
    FixMatch,
    ChangeArt,
    Hide,
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
    config: config::Config,
    http: reqwest::Client,
    /// game id → art on disk.
    pub art: HashMap<String, art::Art>,
    art_meta: HashMap<String, (i64, bool)>,
    art_queue: VecDeque<art::Request>,
    art_queued: HashSet<String>,
    art_in_flight: usize,
    /// Game whose art is fading out behind the selected one.
    pub prev_selected: Option<usize>,
    /// Decoded GPU textures kept warm, with a last-used tick for LRU eviction.
    textures: HashMap<PathBuf, (Option<iced::widget::image::Allocation>, u64)>,
    texture_tick: u64,
    prewarmed_for: usize,
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

    /// The game we launched and are watching.
    pub session: Option<launch::Session>,
    pub status: launch::Status,
    /// "Now playing" overlay: shown flag and its fade.
    pub overlay: bool,
    pub playing: Blend,
    /// First press of "Close game" arms it; a second press within the window closes the game.
    pub stop_armed: Option<Instant>,
    /// Tasks started from input handling, run after the current update.
    pending: Vec<Task<Message>>,

    pub screen: Screen,
    /// Focused button on the game page / row in a list / tile in the art picker.
    pub page_focus: usize,
    pub list_focus: usize,
    pub art_slot: art::Slot,
    pub hide_armed: bool,
    /// SteamGridDB matches for the match screen (None = loading).
    pub candidates: Option<Vec<art::Candidate>>,
    /// Pictures for the art picker (None = loading).
    pub choices: Option<Vec<art::Choice>>,
    /// thumb URL → file on disk.
    pub thumbs: HashMap<String, PathBuf>,
    pub overrides: HashMap<String, art::Override>,
    pub meta: HashMap<String, (meta::Meta, i64)>,
    meta_loading: HashSet<String>,

    pub osk: osk::Osk,
    pub typing_for: TypingFor,
    /// Search: filter tab, results, and whether focus is on the results strip.
    pub filter: usize,
    pub results: Vec<usize>,
    pub on_results: bool,
    pub result_col: usize,
    /// Where Back on the game page goes.
    page_return: Screen,
    /// Name searched on the match screen.
    pub match_term: String,
    pub hidden: Vec<store::Row>,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let mut app = Self::init();
        let scan = app.rescan();
        let art = app.queue_art();
        let warm = app.prewarm();
        (app, Task::batch([scan, art, warm, devtools::boot()]))
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
        let art_rows = store.as_ref().and_then(|s| s.load_art().ok()).unwrap_or_default();
        let mut art = HashMap::new();
        let mut art_meta = HashMap::new();
        for (id, (a, at, key)) in art_rows {
            art.insert(id.clone(), a);
            art_meta.insert(id, (at, key));
        }
        let store_ref = store.as_ref();
        let overrides = store_ref.and_then(|s| s.load_overrides().ok()).unwrap_or_default();
        let meta_cache = store_ref.and_then(|s| s.load_meta().ok()).unwrap_or_default();
        let mut app = Self {
            store,
            config: config::load(),
            http: art::client(),
            art,
            art_meta,
            art_queue: VecDeque::new(),
            art_queued: HashSet::new(),
            art_in_flight: 0,
            prev_selected: None,
            textures: HashMap::new(),
            texture_tick: 0,
            prewarmed_for: usize::MAX,
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
            reduce_motion: false,
            toast: None,
            clock: Local::now(),
            session: None,
            status: launch::Status::Starting,
            overlay: false,
            playing: Blend::new(0.0),
            stop_armed: None,
            pending: Vec::new(),
            screen: Screen::Home,
            page_focus: 0,
            list_focus: 0,
            art_slot: art::Slot::Cover,
            hide_armed: false,
            candidates: None,
            choices: None,
            thumbs: HashMap::new(),
            overrides,
            meta: meta_cache,
            meta_loading: HashSet::new(),
            osk: osk::Osk::default(),
            typing_for: TypingFor::MatchName,
            filter: 0,
            results: Vec::new(),
            on_results: false,
            result_col: 0,
            page_return: Screen::Home,
            match_term: String::new(),
            hidden: Vec::new(),
            games: Vec::new(),
            shelves: Vec::new(),
        };
        app.reduce_motion = app.config.reduce_motion || std::env::var_os("SUPERMAN_REDUCE_MOTION").is_some();
        app.set_library(games);
        app.snap_animations();
        app
    }

    /// Replace the library, keeping focus on the same game if it still exists.
    fn set_library(&mut self, games: Vec<Game>) {
        let keep = self.games.get(self.selected).map(|g| g.id.clone());
        let keep_row = self.shelves.get(self.row).map(|s| s.title);
        self.games = games;
        for g in &mut self.games {
            apply_colors(g, self.art.get(&g.id));
        }
        self.prev_selected = None;
        self.prewarmed_for = usize::MAX;
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

    /// Queue art fetches for games that have none (or incomplete art worth retrying), in shelf order.
    fn queue_art(&mut self) -> Task<Message> {
        if self.store.is_none() {
            return Task::none();
        }
        let now = chrono::Utc::now().timestamp();
        let have_key = self.config.steamgriddb_api_key.is_some();
        let order: Vec<usize> = self.shelves.iter().flat_map(|s| s.games.iter().copied()).collect();
        for gi in order {
            let g = &self.games[gi];
            if self.art_queued.contains(&g.id) {
                continue;
            }
            let needs = match (self.art.get(&g.id), self.art_meta.get(&g.id)) {
                (Some(a), Some(&(at, with_key))) => {
                    let incomplete = a.cover.is_none() || a.hero.is_none() || a.logo.is_none();
                    incomplete && ((have_key && !with_key) || now - at > 3 * 86_400)
                }
                _ => true,
            };
            if needs {
                self.art_queued.insert(g.id.clone());
                let req = self.request(gi);
                self.art_queue.push_back(req);
            }
        }
        self.pump_art()
    }

    fn request(&self, gi: usize) -> art::Request {
        let g = &self.games[gi];
        art::Request {
            game_id: g.id.clone(),
            title: g.title.clone(),
            steam_appid: g.steam_appid,
            pick: self.overrides.get(&g.id).cloned().unwrap_or_default(),
        }
    }

    /// Fetch a game's art again now (after a new match or pinned picture).
    fn refetch(&mut self, gi: usize) -> Task<Message> {
        let req = self.request(gi);
        self.art_queue.retain(|r| r.game_id != req.game_id);
        self.art_queued.insert(req.game_id.clone());
        self.art_queue.push_front(req);
        self.pump_art()
    }

    fn pump_art(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        while self.art_in_flight < 4 {
            let Some(req) = self.art_queue.pop_front() else { break };
            self.art_in_flight += 1;
            let id = req.game_id.clone();
            let key = self.config.steamgriddb_api_key.clone();
            let with_key = key.is_some();
            tasks.push(Task::perform(art::fetch(self.http.clone(), key, req), move |a| Message::ArtFetched(id.clone(), a, with_key)));
        }
        Task::batch(tasks)
    }

    /// Decode and upload the textures near focus ahead of time, and let far-away ones go.
    fn prewarm(&mut self) -> Task<Message> {
        const KEEP: usize = 160;
        if self.prewarmed_for == self.selected || self.shelves.is_empty() {
            return Task::none();
        }
        self.prewarmed_for = self.selected;
        let mut want: Vec<PathBuf> = Vec::new();
        let row_games = &self.shelves[self.row].games;
        let col = self.cols[self.row];
        // Big art for the focused game and its neighbours.
        for &gi in row_games.iter().skip(col.saturating_sub(2)).take(5) {
            if let Some(a) = self.art.get(&self.games[gi].id) {
                want.extend([&a.backdrop, &a.hero, &a.logo].into_iter().flatten().cloned());
            }
        }
        // Covers on the rows around focus.
        for r in self.row.saturating_sub(1)..(self.row + 3).min(self.shelves.len()) {
            let c = self.cols[r];
            for &gi in self.shelves[r].games.iter().skip(c.saturating_sub(3)).take(12) {
                if let Some(p) = self.art.get(&self.games[gi].id).and_then(|a| a.cover.clone()) {
                    want.push(p);
                }
            }
        }
        let mut tasks = Vec::new();
        for path in want {
            self.texture_tick += 1;
            let tick = self.texture_tick;
            if let Some(e) = self.textures.get_mut(&path) {
                e.1 = tick;
                continue;
            }
            self.textures.insert(path.clone(), (None, tick));
            let handle = iced::widget::image::Handle::from_path(&path);
            tasks.push(iced::widget::image::allocate(handle).map(move |r| Message::Allocated(path.clone(), r)));
        }
        if self.textures.len() > KEEP {
            let mut ticks: Vec<u64> = self.textures.values().map(|e| e.1).collect();
            ticks.sort_unstable();
            let cutoff = ticks[self.textures.len() - KEEP];
            self.textures.retain(|_, e| e.1 >= cutoff);
        }
        Task::batch(tasks)
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
        // No disk churn while a game is running.
        if self.store.is_none() || self.scanning || self.session.is_some() {
            return Task::none();
        }
        self.scanning = true;
        Task::perform(
            async { tokio::task::spawn_blocking(sources::scan_all).await.unwrap_or_default() },
            |r| Message::Scanned(Arc::new(r)),
        )
    }

    /// Progress of the crossfade to the selected game, 0..1.
    pub fn fade(&self) -> f32 {
        self.ambient_t.get()
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
            || !self.playing.settled()
            || self.scroll_x.iter().any(|s| !s.settled())
            || self.grow.iter().flatten().any(|g| !g.settled())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let t = self.handle(message);
        if self.pending.is_empty() {
            return t;
        }
        let mut all = std::mem::take(&mut self.pending);
        all.push(t);
        Task::batch(all)
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Pad(PadEvent::Connected(kind)) => self.pad = kind,
            Message::Pad(PadEvent::Disconnected) => self.pad = PadKind::Keyboard,
            Message::Pad(PadEvent::Press(a)) => {
                self.press(a);
                return self.prewarm();
            }
            Message::Pad(PadEvent::Release(a)) => self.release(a),
            Message::Key(a, true) => {
                self.pad = PadKind::Keyboard;
                self.press(a);
                return self.prewarm();
            }
            Message::Key(a, false) => self.release(a),
            Message::RawKey(key, text, pressed) => {
                if pressed && self.typing() && !self.overlay && self.type_key(&key, text.as_deref()) {
                    self.pad = PadKind::Keyboard;
                    return Task::none();
                }
                if let Some(a) = input::map_key(&key) {
                    return self.handle(Message::Key(a, pressed));
                }
            }
            Message::Frame(now) => {
                self.frame(now);
                // Held-direction repeats move focus from here.
                return self.prewarm();
            }
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
                            return Task::batch([self.queue_art(), self.prewarm()]);
                        }
                        Ok(false) => {}
                        Err(e) => eprintln!("saving library failed: {e}"),
                    }
                }
            }
            Message::ArtFetched(id, a, with_key) => {
                self.art_in_flight -= 1;
                self.art_queued.remove(&id);
                if let Some(store) = &self.store {
                    if let Err(e) = store.save_art(&id, &a, with_key) {
                        eprintln!("saving art failed: {e}");
                    }
                }
                self.art_meta.insert(id.clone(), (chrono::Utc::now().timestamp(), with_key));
                if let Some(gi) = self.games.iter().position(|g| g.id == id) {
                    apply_colors(&mut self.games[gi], Some(&a));
                    if gi == self.selected {
                        let g = &self.games[gi];
                        self.ambient_to = (g.ambient, g.accent);
                    }
                }
                self.art.insert(id, a);
                self.prewarmed_for = usize::MAX;
                return Task::batch([self.pump_art(), self.prewarm()]);
            }
            Message::Allocated(path, result) => match result {
                Ok(a) => {
                    if let Some(slot) = self.textures.get_mut(&path) {
                        slot.0 = Some(a);
                    }
                }
                Err(_) => {
                    self.textures.remove(&path);
                }
            },
            Message::Poll => return self.poll(),
            Message::MetaFetched(id, m) => {
                self.meta_loading.remove(&id);
                if let Some(store) = &self.store {
                    let _ = store.save_meta(&id, &m);
                }
                self.meta.insert(id, (m, chrono::Utc::now().timestamp()));
            }
            Message::Candidates(id, list) => {
                if self.screen == Screen::Match && self.games.get(self.selected).is_some_and(|g| g.id == id) {
                    self.candidates = Some(list);
                }
            }
            Message::Choices(id, slot, list) => {
                if self.screen == Screen::Art && self.art_slot == slot && self.games.get(self.selected).is_some_and(|g| g.id == id) {
                    let tasks: Vec<_> = list
                        .iter()
                        .filter(|c| !self.thumbs.contains_key(&c.thumb))
                        .map(|c| {
                            let url = c.thumb.clone();
                            Task::perform(art::thumb(self.http.clone(), url.clone()), move |p| Message::Thumb(url.clone(), p))
                        })
                        .collect();
                    self.choices = Some(list);
                    return Task::batch(tasks);
                }
            }
            Message::Thumb(url, path) => {
                if let Some(p) = path {
                    self.thumbs.insert(url, p);
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
        // A fade, not movement: keep it with reduced motion.
        self.playing.step(dt, false);
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
        if self.overlay {
            match a {
                Action::Back => self.set_overlay(false),
                Action::Details => self.stop_game(),
                _ => {}
            }
            return;
        }
        match self.screen {
            Screen::Home => {}
            Screen::Game => return self.act_page(a),
            Screen::Match => return self.act_match(a),
            Screen::Art => return self.act_art(a),
            Screen::Search => return self.act_search(a),
            Screen::Typing => return self.act_typing(a),
            Screen::Settings => return self.act_settings(a),
            Screen::Hidden => return self.act_hidden(a),
        }
        match (self.focus, a) {
            (_, Action::Search) => self.open_search(),
            (_, Action::Menu) => self.open_settings(),
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
            self.prev_selected = (self.selected < self.games.len()).then_some(self.selected);
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
        if let Some(sess) = &self.session {
            if sess.game_id == g.id {
                self.set_overlay(true);
            } else {
                let msg = format!("{} is still running. Close it before starting another game.", sess.title);
                self.notice(&msg);
            }
            return;
        }
        let title = g.title.clone();
        match launch::start(&g.id, &g.title, &g.launch) {
            Ok(sess) => {
                let now = Local::now();
                if let Some(store) = &self.store {
                    let _ = store.mark_played(&g.id, now.timestamp());
                }
                self.games[self.selected].last_played = Some(now);
                self.session = Some(sess);
                self.status = launch::Status::Starting;
                self.stop_armed = None;
                self.set_overlay(true);
            }
            Err(e) => self.notice(&format!("{title} didn't start: {e}")),
        }
    }

    fn set_overlay(&mut self, on: bool) {
        self.overlay = on;
        self.playing.set(if on { 1.0 } else { 0.0 });
        self.stop_armed = None;
    }

    fn stop_game(&mut self) {
        let Some(sess) = &mut self.session else { return };
        if self.stop_armed.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
            sess.stop();
            self.stop_armed = None;
        } else {
            self.stop_armed = Some(Instant::now());
        }
    }

    /// Follow the running game; when it closes, record playtime and come back to the same card.
    fn poll(&mut self) -> Task<Message> {
        let Some(sess) = &mut self.session else { return Task::none() };
        self.status = sess.poll();
        if self.stop_armed.is_some_and(|t| t.elapsed() >= Duration::from_secs(3)) {
            self.stop_armed = None;
        }
        let (id, title, stopped) = (sess.game_id.clone(), sess.title.clone(), sess.stopped);
        let msg = match self.status {
            launch::Status::Starting | launch::Status::Running => return Task::none(),
            launch::Status::Ended(d) => {
                let mins = ((d.as_secs() + 30) / 60) as u32;
                if mins > 0 {
                    if let Some(store) = &self.store {
                        let _ = store.add_playtime(&id, mins);
                    }
                    if let Some(g) = self.games.iter_mut().find(|g| g.id == id) {
                        g.minutes_played += mins;
                    }
                }
                let now = Local::now();
                if let Some(store) = &self.store {
                    let _ = store.mark_played(&id, now.timestamp());
                }
                if let Some(g) = self.games.iter_mut().find(|g| g.id == id) {
                    g.last_played = Some(now);
                }
                match mins {
                    0 => format!("Closed {title}"),
                    1 => format!("Played {title} for 1 minute"),
                    m if m < 120 => format!("Played {title} for {m} minutes"),
                    m => format!("Played {title} for {} hours {} minutes", m / 60, m % 60),
                }
            }
            launch::Status::Lost if stopped => format!("Closed {title}"),
            launch::Status::Lost => format!("Lost track of {title}. If it opened in its own launcher, it's still running there."),
        };
        self.session = None;
        self.set_overlay(false);
        self.notice(&msg);
        // Shelves were frozen while playing; catch up, and bring our window back.
        let rescan = self.rescan();
        Task::batch([rescan, window::latest().and_then(window::gain_focus)])
    }

    fn details(&mut self) {
        if self.games.get(self.selected).is_none() {
            return;
        }
        self.screen = Screen::Game;
        self.page_return = Screen::Home;
        self.page_focus = 0;
        self.hide_armed = false;
        let g = &self.games[self.selected];
        let stale = self.meta.get(&g.id).is_none_or(|(_, at)| chrono::Utc::now().timestamp() - at > 14 * 86_400);
        if let Some(appid) = g.steam_appid.filter(|_| stale && !self.meta_loading.contains(&g.id) && self.store.is_some()) {
            let id = g.id.clone();
            self.meta_loading.insert(id.clone());
            self.pending.push(Task::perform(meta::fetch(self.http.clone(), appid), move |m| Message::MetaFetched(id.clone(), m)));
        }
    }

    pub fn page_buttons(&self) -> Vec<(PageButton, &'static str)> {
        let g = &self.games[self.selected];
        let running = self.session.as_ref().is_some_and(|s| s.game_id == g.id);
        vec![
            (PageButton::Play, if running { "Now playing" } else { "Play" }),
            (PageButton::Favourite, if g.favorite { "Unfavourite" } else { "Favourite" }),
            (PageButton::FixMatch, "Fix match"),
            (PageButton::ChangeArt, "Change art"),
            (PageButton::Hide, if self.hide_armed { "Press again to hide" } else { "Hide game" }),
        ]
    }

    fn act_page(&mut self, a: Action) {
        let n = self.page_buttons().len();
        if a != Action::Confirm {
            self.hide_armed = false;
        }
        match a {
            Action::Left => self.page_focus = self.page_focus.saturating_sub(1),
            Action::Right => self.page_focus = (self.page_focus + 1).min(n - 1),
            Action::Back => {
                self.screen = self.page_return;
                if self.screen == Screen::Home {
                    self.focus_game(self.selected);
                }
            }
            Action::Confirm => {
                let (b, _) = self.page_buttons()[self.page_focus];
                if b != PageButton::Hide {
                    self.hide_armed = false;
                }
                self.page_action(b);
            }
            _ => {}
        }
    }

    fn page_action(&mut self, b: PageButton) {
        let has_key = self.config.steamgriddb_api_key.clone();
        let g = self.games[self.selected].clone();
        match b {
            PageButton::Play => self.play(),
            PageButton::Favourite => {
                let on = !g.favorite;
                if let Some(store) = &self.store {
                    let _ = store.set_favorite(&g.copies, on);
                }
                self.games[self.selected].favorite = on;
                self.reshelve();
            }
            PageButton::FixMatch | PageButton::ChangeArt if has_key.is_none() => {
                self.notice("Add a SteamGridDB API key to config.toml to pick art.");
            }
            PageButton::FixMatch => self.search_matches(g.title.clone()),
            PageButton::ChangeArt => {
                self.screen = Screen::Art;
                self.open_slot(art::Slot::Cover);
            }
            PageButton::Hide if !self.hide_armed => self.hide_armed = true,
            PageButton::Hide => {
                if let Some(store) = &self.store {
                    let _ = store.set_hidden(&g.copies, true);
                }
                self.hide_armed = false;
                self.screen = Screen::Home;
                let games: Vec<Game> = self.games.iter().filter(|x| x.id != g.id).cloned().collect();
                self.set_library(games);
                self.notice(&format!("Hid {}", g.title));
            }
        }
    }

    /// Rebuild shelves after a change to the current games (favourite), keeping focus.
    fn reshelve(&mut self) {
        let games = std::mem::take(&mut self.games);
        self.set_library(games);
    }

    fn open_slot(&mut self, slot: art::Slot) {
        self.art_slot = slot;
        self.list_focus = 0;
        self.choices = None;
        let key = self.config.steamgriddb_api_key.clone().unwrap_or_default();
        let req = self.request(self.selected);
        let id = req.game_id.clone();
        self.pending.push(Task::perform(art::choices(self.http.clone(), key, req, slot), move |l| Message::Choices(id.clone(), slot, l)));
    }

    fn act_match(&mut self, a: Action) {
        // Rows: each candidate, "Search by another name", then "None of these".
        let n = self.candidates.as_ref().map_or(0, |c| c.len() + 2);
        let c_len = n.saturating_sub(2);
        if a == Action::Confirm && n > 0 && self.list_focus == c_len {
            self.typing_for = TypingFor::MatchName;
            self.osk = osk::Osk::new(&self.games[self.selected].title, false);
            self.screen = Screen::Typing;
            return;
        }
        match a {
            Action::Up => self.list_focus = self.list_focus.saturating_sub(1),
            Action::Down if n > 0 => self.list_focus = (self.list_focus + 1).min(n - 1),
            Action::Back => self.screen = Screen::Game,
            Action::Confirm if n > 0 => {
                let pick = self.candidates.as_ref().and_then(|c| c.get(self.list_focus)).map_or(0, |c| c.id);
                let id = self.games[self.selected].id.clone();
                // A new match makes earlier pinned pictures meaningless.
                let o = art::Override { sgdb_id: Some(pick), pins: Default::default() };
                self.set_override(&id, o);
                self.screen = Screen::Game;
                let t = self.refetch(self.selected);
                self.pending.push(t);
                self.notice(if pick == 0 { "Using a title card for this game" } else { "Matched. Fetching art" });
            }
            _ => {}
        }
    }

    fn act_art(&mut self, a: Action) {
        // Tiles: "Automatic", then each choice.
        let n = self.choices.as_ref().map_or(0, |c| c.len() + 1);
        let slots = art::Slot::ALL;
        let si = slots.iter().position(|&s| s == self.art_slot).unwrap_or(0);
        match a {
            Action::Left => self.list_focus = self.list_focus.saturating_sub(1),
            Action::Right if n > 0 => self.list_focus = (self.list_focus + 1).min(n - 1),
            Action::PrevTab | Action::Up if si > 0 => self.open_slot(slots[si - 1]),
            Action::NextTab | Action::Down if si + 1 < slots.len() => self.open_slot(slots[si + 1]),
            Action::Back => self.screen = Screen::Game,
            Action::Confirm if n > 0 => {
                let url = if self.list_focus == 0 { None } else { self.choices.as_ref().and_then(|c| c.get(self.list_focus - 1)).map(|c| c.url.clone()) };
                let id = self.games[self.selected].id.clone();
                let mut o = self.overrides.get(&id).cloned().unwrap_or_default();
                o.pins[si] = url;
                self.set_override(&id, o);
                let t = self.refetch(self.selected);
                self.pending.push(t);
                self.notice(&format!("{} updated", self.art_slot.label()));
            }
            _ => {}
        }
    }

    fn search_matches(&mut self, term: String) {
        self.screen = Screen::Match;
        self.list_focus = 0;
        self.candidates = None;
        self.match_term = term.clone();
        let id = self.games[self.selected].id.clone();
        let key = self.config.steamgriddb_api_key.clone().unwrap_or_default();
        self.pending.push(Task::perform(art::search(self.http.clone(), key, term), move |l| Message::Candidates(id.clone(), l)));
    }

    /// Put shelf focus on a game (after visiting it from search).
    fn focus_game(&mut self, gi: usize) {
        let found = self.shelves.iter().enumerate().find_map(|(r, s)| s.games.iter().position(|&g| g == gi).map(|c| (r, c)));
        if let Some((r, c)) = found {
            self.row = r;
            self.cols[r] = c;
            self.focus = Focus::Shelf;
        }
        self.sync_focus();
    }

    pub fn typing(&self) -> bool {
        matches!(self.screen, Screen::Search | Screen::Typing)
    }

    /// Physical keyboard typing. Returns true if the key was used.
    fn type_key(&mut self, key: &keyboard::Key, text: Option<&str>) -> bool {
        use keyboard::key::Named;
        let out = match key {
            keyboard::Key::Named(Named::Backspace) => self.osk.delete(),
            keyboard::Key::Named(Named::Enter) => osk::Out::Done,
            keyboard::Key::Named(Named::Escape) => return false,
            _ => match text.filter(|t| !t.is_empty() && t.chars().all(|c| !c.is_control())) {
                Some(t) => {
                    for c in t.chars() {
                        self.osk.push(c);
                    }
                    osk::Out::Changed
                }
                None => return false,
            },
        };
        self.osk_out(out);
        true
    }

    /// React to the keyboard's result on whichever screen owns it.
    fn osk_out(&mut self, out: osk::Out) {
        match (self.screen, out) {
            (Screen::Search, osk::Out::Changed) => self.refresh_results(),
            (Screen::Search, osk::Out::Done | osk::Out::LeaveUp) if !self.results.is_empty() => self.on_results = true,
            (Screen::Typing, osk::Out::Done) => self.finish_typing(),
            _ => {}
        }
    }

    /// Pad shortcuts shared by every keyboard screen. Returns true if used.
    fn osk_pad(&mut self, a: Action) -> bool {
        let out = match a {
            Action::Up => self.osk.up(),
            Action::Down => {
                self.osk.down();
                osk::Out::Nothing
            }
            Action::Left => {
                self.osk.left();
                osk::Out::Nothing
            }
            Action::Right => {
                self.osk.right();
                osk::Out::Nothing
            }
            Action::Confirm => self.osk.press(),
            Action::Details => self.osk.delete(),
            Action::Search => self.osk.push(' '),
            Action::Menu => osk::Out::Done,
            _ => return false,
        };
        self.osk_out(out);
        true
    }

    fn open_search(&mut self) {
        self.screen = Screen::Search;
        self.osk = osk::Osk::new("", false);
        self.on_results = false;
        self.refresh_results();
    }

    /// Filter tabs that have games, as indices into `library::FILTERS`.
    pub fn filters(&self) -> Vec<usize> {
        (0..library::FILTERS.len()).filter(|&i| self.games.iter().any(library::FILTERS[i].filter)).collect()
    }

    fn refresh_results(&mut self) {
        let f = &library::FILTERS[self.filter.min(library::FILTERS.len() - 1)];
        self.results = library::search(&self.games, &self.osk.text, f);
        self.result_col = 0;
        if self.results.is_empty() {
            self.on_results = false;
        }
    }

    fn act_search(&mut self, a: Action) {
        match a {
            Action::PrevTab | Action::NextTab => {
                let tabs = self.filters();
                let i = tabs.iter().position(|&t| t == self.filter).unwrap_or(0);
                let j = if a == Action::NextTab { (i + 1).min(tabs.len() - 1) } else { i.saturating_sub(1) };
                self.filter = tabs.get(j).copied().unwrap_or(0);
                self.refresh_results();
            }
            Action::Back if self.on_results => self.on_results = false,
            Action::Back => {
                self.screen = Screen::Home;
                self.sync_focus();
            }
            _ if self.on_results => match a {
                Action::Left => self.result_col = self.result_col.saturating_sub(1),
                Action::Right => self.result_col = (self.result_col + 1).min(self.results.len().saturating_sub(1)),
                Action::Down => self.on_results = false,
                Action::Confirm | Action::Details => {
                    if let Some(&gi) = self.results.get(self.result_col) {
                        if gi != self.selected {
                            self.prev_selected = None;
                            self.selected = gi;
                            let g = &self.games[gi];
                            self.ambient_to = (g.ambient, g.accent);
                            self.ambient_from = self.ambient_to;
                            self.prewarmed_for = usize::MAX;
                        }
                        self.details();
                        self.page_return = Screen::Search;
                    }
                }
                _ => {}
            },
            _ => {
                self.osk_pad(a);
            }
        }
    }

    fn finish_typing(&mut self) {
        let text = self.osk.text.trim().to_owned();
        match self.typing_for {
            TypingFor::MatchName if !text.is_empty() => self.search_matches(text),
            TypingFor::MatchName => self.screen = Screen::Match,
            TypingFor::ApiKey => {
                let key = (!text.is_empty()).then(|| text.clone());
                match config::update(|c| c.steamgriddb_api_key = key.clone()) {
                    Ok(()) => {
                        let had = self.config.steamgriddb_api_key.is_some();
                        self.config.steamgriddb_api_key = key;
                        self.notice(if self.config.steamgriddb_api_key.is_some() { "Saved the SteamGridDB key" } else { "Removed the SteamGridDB key" });
                        if !had {
                            let t = self.queue_art();
                            self.pending.push(t);
                        }
                    }
                    Err(e) => self.notice(&format!("Couldn't save settings: {e}")),
                }
                self.screen = Screen::Settings;
            }
        }
    }

    fn act_typing(&mut self, a: Action) {
        if a == Action::Back {
            self.screen = match self.typing_for {
                TypingFor::MatchName => Screen::Match,
                TypingFor::ApiKey => Screen::Settings,
            };
            return;
        }
        self.osk_pad(a);
    }

    fn open_settings(&mut self) {
        self.screen = Screen::Settings;
        self.list_focus = 0;
    }

    pub fn settings_rows(&self) -> Vec<(SettingRow, &'static str, String)> {
        let key = match &self.config.steamgriddb_api_key {
            Some(k) if k.len() > 4 => format!("Set, ending {}", &k[k.len() - 4..]),
            Some(_) => "Set".into(),
            None => "Not set. Needed for art of non-Steam games".into(),
        };
        let hidden = self.store.as_ref().and_then(|s| s.load().ok()).map_or(0, |r| r.iter().filter(|r| r.hidden).count());
        vec![
            (SettingRow::ApiKey, "SteamGridDB API key", key),
            (SettingRow::ReduceMotion, "Reduce motion", if self.reduce_motion { "On".into() } else { "Off".into() }),
            (SettingRow::Rescan, "Look for new games", if self.scanning { "Looking…".into() } else { "Checks every launcher now".into() }),
            (SettingRow::Hidden, "Hidden games", if hidden == 0 { "None".into() } else { format!("{hidden}") }),
        ]
    }

    fn act_settings(&mut self, a: Action) {
        let rows = self.settings_rows();
        match a {
            Action::Up => self.list_focus = self.list_focus.saturating_sub(1),
            Action::Down => self.list_focus = (self.list_focus + 1).min(rows.len() - 1),
            Action::Back | Action::Menu => {
                self.screen = Screen::Home;
                self.sync_focus();
            }
            Action::Confirm => match rows[self.list_focus].0 {
                SettingRow::ApiKey => {
                    self.typing_for = TypingFor::ApiKey;
                    let cur = self.config.steamgriddb_api_key.clone().unwrap_or_default();
                    self.osk = osk::Osk::new(&cur, true);
                    self.screen = Screen::Typing;
                }
                SettingRow::ReduceMotion => {
                    let on = !self.reduce_motion;
                    self.reduce_motion = on;
                    if let Err(e) = config::update(|c| c.reduce_motion = on) {
                        self.notice(&format!("Couldn't save settings: {e}"));
                    }
                }
                SettingRow::Rescan => {
                    let t = self.rescan();
                    self.pending.push(t);
                }
                SettingRow::Hidden => {
                    self.load_hidden();
                    self.screen = Screen::Hidden;
                    self.list_focus = 0;
                }
            },
            _ => {}
        }
    }

    fn load_hidden(&mut self) {
        self.hidden = self.store.as_ref().and_then(|s| s.load().ok()).unwrap_or_default().into_iter().filter(|r| r.hidden).collect();
        self.hidden.sort_by_cached_key(|r| r.title.to_lowercase());
    }

    fn act_hidden(&mut self, a: Action) {
        match a {
            Action::Up => self.list_focus = self.list_focus.saturating_sub(1),
            Action::Down => self.list_focus = (self.list_focus + 1).min(self.hidden.len().saturating_sub(1)),
            Action::Back => {
                self.screen = Screen::Settings;
                self.list_focus = 3;
            }
            Action::Confirm => {
                let Some(r) = self.hidden.get(self.list_focus) else { return };
                let (id, title) = (r.id.clone(), r.title.clone());
                if let Some(store) = &self.store {
                    let _ = store.set_hidden(&[id], false);
                    let games = library::from_rows(store.load().unwrap_or_default());
                    self.set_library(games);
                }
                self.load_hidden();
                self.list_focus = self.list_focus.min(self.hidden.len().saturating_sub(1));
                self.notice(&format!("{title} is back in your library"));
                let t = self.queue_art();
                self.pending.push(t);
            }
            _ => {}
        }
    }

    fn set_override(&mut self, id: &str, o: art::Override) {
        if let Some(store) = &self.store {
            let _ = store.save_override(id, &o);
        }
        self.overrides.insert(id.to_owned(), o);
    }

    fn notice(&mut self, text: &str) {
        self.toast = Some((text.to_owned(), Instant::now() + Duration::from_millis(2600)));
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            // Only while a game is running.
            if self.session.is_some() { time::every(Duration::from_secs(1)).map(|_| Message::Poll) } else { Subscription::none() },
            input::gamepads().map(Message::Pad),
            keyboard::listen().filter_map(|e| match e {
                keyboard::Event::KeyPressed { key, text, repeat: false, .. } => Some(Message::RawKey(key, text.map(|t| t.to_string()), true)),
                // Held keys repeat for typing (backspace) but not for navigation.
                keyboard::Event::KeyPressed { key: key @ keyboard::Key::Named(keyboard::key::Named::Backspace), repeat: true, .. } => {
                    Some(Message::RawKey(key, None, true))
                }
                keyboard::Event::KeyReleased { key, .. } => Some(Message::RawKey(key, None, false)),
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
        let base = match self.screen {
            Screen::Home => ui::home::view(self),
            Screen::Game => ui::game::view(self),
            Screen::Match => ui::game::matches(self),
            Screen::Art => ui::game::art_picker(self),
            Screen::Search => ui::search::view(self),
            Screen::Typing => ui::search::typing(self),
            Screen::Settings => ui::settings::view(self),
            Screen::Hidden => ui::settings::hidden(self),
        };
        let mut layers = iced::widget::stack![base];
        if self.playing.get() > 0.005 {
            layers = layers.push(ui::playing::view(self));
        }
        if let Some((msg, _)) = &self.toast {
            layers = layers.push(ui::home::toast(msg));
        }
        layers.into()
    }
}

fn apply_colors(g: &mut Game, art: Option<&art::Art>) {
    let c = |hex: u32| Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
    if let Some(a) = art {
        if let (Some(amb), Some(acc)) = (a.ambient, a.accent) {
            g.ambient = c(amb);
            g.accent = c(acc);
        }
    }
}
