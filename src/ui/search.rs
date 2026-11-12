//! Search: filter tabs, the query, matching covers, and the on-screen keyboard.
//! Also the plain text-entry screen used for match names and the API key.

use iced::border::Border;
use iced::font::Weight;
use iced::widget::{Space, column, container, pin, row, text};
use iced::{Element, Fill, Point, Shadow, Vector, alignment};

use super::game::screen;
use super::home::{cover_tile, glyph};
use crate::input::Action;
use crate::library::FILTERS;
use crate::osk::{ACTIONS, Key, ROWS};
use crate::theme::{DUSK, HAZE, MARQUEE, NIGHT, PAPER, alpha, body, display, size};
use crate::{App, Message, TypingFor};

const KEY_W: f32 = 72.0;
const KEY_H: f32 = 60.0;
const KEY_GAP: f32 = 8.0;
const RESULT_W: f32 = 180.0;
const RESULT_H: f32 = 270.0;
const RESULT_GAP: f32 = 24.0;

const HINTS: &[(Action, &str)] = &[
    (Action::Confirm, "Type"),
    (Action::Details, "Delete"),
    (Action::Search, "Space"),
    (Action::Menu, "Done"),
    (Action::Back, "Back"),
];

pub fn view(app: &App) -> Element<'_, Message> {
    let mut tabs = row![glyph(app.glyph(Action::PrevTab))].spacing(28).align_y(alignment::Vertical::Center);
    for i in app.filters() {
        let on = i == app.filter;
        tabs = tabs.push(text(FILTERS[i].label).size(size::S).font(body(if on { Weight::Bold } else { Weight::Normal })).color(if on { PAPER } else { HAZE }));
    }
    tabs = tabs.push(glyph(app.glyph(Action::NextTab)));

    let count = match app.results.len() {
        0 => "No games match".to_owned(),
        1 => "1 game".to_owned(),
        n => format!("{n} games"),
    };
    let head = column![tabs, Space::new().height(28), row![query(app, "Type a game's name"), Space::new().width(Fill), text(count).size(size::S).color(HAZE)].align_y(alignment::Vertical::Bottom)];

    let content = column![
        Space::new().height(110),
        head,
        Space::new().height(28),
        container(results(app)).height(RESULT_H * 1.12 + 10.0),
        Space::new().height(28),
        keyboard(app, !app.on_results),
    ]
    .padding([0.0, crate::theme::MARGIN]);

    let hints: &[(Action, &str)] = if app.on_results {
        &[(Action::Confirm, "Open"), (Action::Down, "Keyboard"), (Action::NextTab, "Filter"), (Action::Back, "Keyboard")]
    } else {
        hints_for(app)
    };
    screen(app, content.into(), hints)
}

/// The typed text, or a quiet placeholder, with a caret.
fn query<'a>(app: &App, placeholder: &'a str) -> Element<'a, Message> {
    let shown = if app.osk.mask {
        let n = app.osk.text.chars().count();
        let tail: String = app.osk.text.chars().skip(n.saturating_sub(4)).collect();
        "•".repeat(n.saturating_sub(4)) + &tail
    } else {
        app.osk.text.clone()
    };
    if shown.is_empty() {
        return row![caret(), text(placeholder).size(size::L).font(display(Weight::Semibold)).color(alpha(HAZE, 0.8))].spacing(6).into();
    }
    row![text(shown).size(size::L).font(display(Weight::ExtraBold)), caret()].spacing(4).align_y(alignment::Vertical::Center).into()
}

fn caret<'a>() -> Element<'a, Message> {
    container(Space::new()).width(3).height(44).style(|_| container::Style { background: Some(MARQUEE.into()), ..Default::default() }).into()
}

fn results(app: &App) -> Element<'_, Message> {
    let focus_x = app.result_col as f32 * (RESULT_W + RESULT_GAP);
    let scroll = (focus_x - 700.0).max(0.0);
    let mut tiles: Vec<Element<'_, Message>> = Vec::new();
    for (i, &gi) in app.results.iter().enumerate() {
        let x = i as f32 * (RESULT_W + RESULT_GAP) - scroll;
        if x < -RESULT_W || x > 2000.0 {
            continue;
        }
        let f = if app.on_results && i == app.result_col { 1.0 } else { 0.0 };
        let s = 1.0 + 0.1 * f;
        let (w, h) = (RESULT_W * s, RESULT_H * s);
        tiles.push(pin(super::home::clickable(cover_tile(app, gi, w, h, f, if app.on_results { 0.0 } else { 0.25 }), Message::Point(crate::Target::Result(i)))).position(Point::new(x - (w - RESULT_W) / 2.0, (RESULT_H * 1.1 - h) / 2.0)).into());
    }
    iced::widget::Stack::with_children(tiles).width(Fill).height(Fill).into()
}

/// The pad keyboard. `active` = focus is on it (otherwise no key is highlighted).
pub fn keyboard(app: &App, active: bool) -> Element<'_, Message> {
    let k = &app.osk;
    let mut rows = column![].spacing(KEY_GAP);
    for (r, chars) in ROWS.iter().enumerate() {
        let mut line = row![].spacing(KEY_GAP);
        for c in 0..chars.chars().count() {
            line = line.push(key(k.key_at(r, c), 1.0, active && k.row == r && k.col == c, k.caps));
        }
        rows = rows.push(line);
    }
    let mut line = row![].spacing(KEY_GAP);
    for (c, &(kk, w)) in ACTIONS.iter().enumerate() {
        line = line.push(key(kk, w, active && k.row == ROWS.len() && k.col == c, k.caps));
    }
    rows.push(line).into()
}

fn key<'a>(k: Key, units: f32, focused: bool, caps: bool) -> Element<'a, Message> {
    let label = match k {
        Key::Char(c) => c.to_string(),
        Key::Shift => "Shift".into(),
        Key::Space => "Space".into(),
        Key::Delete => "Delete".into(),
        Key::Done => "Done".into(),
    };
    let lit = k == Key::Shift && caps;
    let w = KEY_W * units + KEY_GAP * (units - 1.0);
    let fg = if focused { NIGHT } else { PAPER };
    container(text(label).size(if matches!(k, Key::Char(_)) { size::S } else { size::XS }).font(body(Weight::Bold)).color(fg))
        .width(w)
        .height(KEY_H)
        .center_x(w)
        .center_y(KEY_H)
        .style(move |_| container::Style {
            background: Some(if focused { MARQUEE.into() } else { alpha(DUSK, if lit { 1.0 } else { 0.7 }).into() }),
            border: Border { radius: 10.0.into(), width: if lit { 1.0 } else { 0.0 }, color: alpha(PAPER, 0.5) },
            shadow: Shadow { color: alpha(MARQUEE, if focused { 0.45 } else { 0.0 }), offset: Vector::ZERO, blur_radius: 22.0 },
            ..Default::default()
        })
        .into()
}

/// Text entry for a match name or the API key.
pub fn typing(app: &App) -> Element<'_, Message> {
    let (title, help, placeholder) = match app.typing_for {
        TypingFor::MatchName => ("Search by another name", "Try the name the game has on its store page.", "Game name"),
        TypingFor::ApiKey => (
            "SteamGridDB API key",
            "Get a free key at steamgriddb.com/profile/preferences/api. It stays in your config file on this computer.",
            "Paste or type the key",
        ),
    };
    let content = column![
        Space::new().height(150),
        text(title).size(size::XL).font(display(Weight::ExtraBold)),
        Space::new().height(14),
        text(help).size(size::S).color(HAZE).width(1000),
        Space::new().height(48),
        query(app, placeholder),
        Space::new().height(48),
        keyboard(app, true),
    ]
    .padding([0.0, crate::theme::MARGIN]);
    screen(app, content.into(), hints_for(app))
}

/// On a physical keyboard, letters type, so only Enter and Esc are worth showing.
fn hints_for(app: &App) -> &'static [(Action, &'static str)] {
    if app.pad == crate::input::PadKind::Keyboard { &[(Action::Confirm, "Done"), (Action::Back, "Back")] } else { HINTS }
}
