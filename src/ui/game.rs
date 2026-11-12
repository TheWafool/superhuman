//! Game page, "Which game is this?" and the art picker.

use iced::border::Border;
use iced::font::Weight;
use iced::widget::image::Handle;
use iced::widget::{Space, column, container, image, pin, row, stack, text};
use iced::{Background, ContentFit, Element, Fill, Point, Shadow, Shrink, Vector, alignment};

use super::home::{FOOTER_H, LOGO_H, art_layers, glyph, hero_button, top_bar};
use crate::art::Slot;
use crate::input::Action;
use crate::theme::{DUSK, HAZE, MARGIN, MARQUEE, NIGHT, PAPER, alpha, body, display, size};
use crate::{App, Message};

/// Height of the details band under the buttons.
const BODY_H: f32 = 300.0;
/// The description column stays under ~70 characters a line.
const DESC_W: f32 = 780.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let game = &app.games[app.selected];

    let logo = app.art.get(&game.id).and_then(|a| a.logo.as_ref());
    let title: Element<'_, Message> = match logo {
        Some(p) => container(image(Handle::from_path(p)).height(LOGO_H).width(Shrink).content_fit(ContentFit::Contain))
            .max_width(600)
            .height(LOGO_H)
            .align_y(alignment::Vertical::Bottom)
            .into(),
        None => text(&game.title).size(size::XXL).font(display(Weight::ExtraBold)).line_height(1.0).width(1100).into(),
    };

    let running = app.session.as_ref().is_some_and(|s| s.game_id == game.id);
    let mut meta = row![text(if running { "Running now".to_owned() } else { game.last_played_label() }).size(size::S).color(alpha(PAPER, 0.9))].spacing(36);
    if let Some(p) = game.playtime_label() {
        meta = meta.push(text(p).size(size::S).color(HAZE));
    }
    meta = meta.push(text(game.sources_label()).size(size::S).color(HAZE));

    let mut buttons = row![].spacing(20);
    for (i, (_, label)) in app.page_buttons().into_iter().enumerate() {
        buttons = buttons.push(hero_button(label, i == app.page_focus, 1.0));
    }

    let content = column![
        Space::new().height(Fill),
        title,
        Space::new().height(18),
        meta,
        Space::new().height(28),
        buttons,
        Space::new().height(56),
        container(details(app)).height(BODY_H),
    ]
    .padding(iced::Padding { left: MARGIN, right: MARGIN, ..Default::default() });

    screen(app, content.into(), &[(Action::Confirm, "Select"), (Action::Back, "Back")])
}

/// Description on the left, facts on the right.
fn details(app: &App) -> Element<'_, Message> {
    let game = &app.games[app.selected];
    let m = app.meta.get(&game.id).map(|(m, _)| m);
    let desc = match (m.and_then(|m| m.description.as_deref()), game.steam_appid) {
        (Some(d), _) => d.to_owned(),
        (None, Some(_)) if m.is_none() => "Loading details…".into(),
        _ => "No store details for this game.".into(),
    };
    let desc = text(desc).size(size::S).line_height(1.5).color(alpha(PAPER, 0.85)).width(DESC_W);

    let mut items: Vec<Element<'_, Message>> = Vec::new();
    let mut fact = |label: &'static str, value: Option<String>| {
        if let Some(v) = value {
            items.push(column![text(label).size(size::XS).color(HAZE), text(v).size(size::S).font(body(Weight::Bold))].spacing(4).into());
        }
    };
    if let Some(m) = m {
        fact("On Linux", m.protondb_label());
        fact("Developer", m.developer.clone());
        fact("Released", m.release.clone());
        fact("Genres", (!m.genres.is_empty()).then(|| m.genres.join(", ")));
    }
    let facts = iced::widget::Column::with_children(items).spacing(18).width(Fill);
    row![desc, Space::new().width(96), facts].into()
}

pub fn matches(app: &App) -> Element<'_, Message> {
    let game = &app.games[app.selected];
    let head = column![
        text("Which game is this?").size(size::XL).font(display(Weight::ExtraBold)),
        text(format!("SteamGridDB results for \u{201C}{}\u{201D}. The one you pick decides its art.", game.title)).size(size::S).color(HAZE),
    ]
    .spacing(14);

    let mut list = column![].spacing(10);
    match &app.candidates {
        None => list = list.push(text("Searching…").size(size::S).color(HAZE)),
        Some(c) => {
            let mut rows: Vec<(String, Option<String>)> = c.iter().map(|c| (c.name.clone(), c.year.map(|y| y.to_string()))).collect();
            rows.push(("None of these. Use a title card".into(), None));
            // A window of rows around focus.
            let start = app.list_focus.saturating_sub(4).min(rows.len().saturating_sub(9));
            for (i, (name, year)) in rows.into_iter().enumerate().skip(start).take(9) {
                list = list.push(list_row(name, year, i == app.list_focus));
            }
        }
    }

    let content = column![Space::new().height(150), head, Space::new().height(40), list].padding([0.0, MARGIN]);
    screen(app, content.into(), &[(Action::Confirm, "Pick"), (Action::Back, "Back")])
}

fn list_row<'a>(name: String, year: Option<String>, focused: bool) -> Element<'a, Message> {
    let fg = if focused { NIGHT } else { PAPER };
    let mut r = row![text(name).size(size::S).font(body(Weight::Bold)).color(fg)].spacing(20).align_y(alignment::Vertical::Center);
    if let Some(y) = year {
        r = r.push(text(y).size(size::S).color(if focused { alpha(NIGHT, 0.7) } else { HAZE }));
    }
    container(r)
        .padding([14.0, 28.0])
        .width(900)
        .style(move |_| container::Style {
            background: Some(if focused { MARQUEE.into() } else { alpha(DUSK, 0.55).into() }),
            border: Border { radius: 12.0.into(), ..Default::default() },
            shadow: Shadow { color: alpha(MARQUEE, if focused { 0.4 } else { 0.0 }), offset: Vector::ZERO, blur_radius: 24.0 },
            ..Default::default()
        })
        .into()
}

/// Tile size for each slot, at the 1080p design size.
fn tile(slot: Slot) -> (f32, f32) {
    match slot {
        Slot::Cover => (200.0, 300.0),
        Slot::Hero => (520.0, 168.0),
        Slot::Logo => (360.0, 168.0),
    }
}

pub fn art_picker<'a>(app: &'a App) -> Element<'a, Message> {
    let mut tabs = row![glyph(app.pad.glyph(Action::PrevTab))].spacing(28).align_y(alignment::Vertical::Center);
    for s in Slot::ALL {
        let on = s == app.art_slot;
        tabs = tabs.push(text(s.label()).size(size::M).font(body(if on { Weight::Bold } else { Weight::Normal })).color(if on { PAPER } else { HAZE }));
    }
    tabs = tabs.push(glyph(app.pad.glyph(Action::NextTab)));

    let head = column![text("Change art").size(size::XL).font(display(Weight::ExtraBold)), Space::new().height(20), tabs];

    let (w, h) = tile(app.art_slot);
    let gap = 24.0;
    let pinned = app.overrides.get(&app.games[app.selected].id).and_then(|o| {
        let i = Slot::ALL.iter().position(|&s| s == app.art_slot)?;
        o.pins[i].clone()
    });
    let strip: Element<'_, Message> = match &app.choices {
        None => text("Loading pictures…").size(size::S).color(HAZE).into(),
        Some(c) if c.is_empty() => text("SteamGridDB has no pictures for this game. Try Fix match first.").size(size::S).color(HAZE).into(),
        Some(c) => {
            // Keep focus in view: scroll once it passes the middle of the screen.
            let focus_x = app.list_focus as f32 * (w + gap);
            let scroll = (focus_x - 700.0).max(0.0);
            let mut tiles: Vec<Element<'a, Message>> = Vec::new();
            let mut push = |i: usize, el: Element<'a, Message>, in_use: bool| {
                let x = i as f32 * (w + gap) - scroll;
                if x > -w && x < 2000.0 {
                    let mut col = column![el].spacing(10);
                    if in_use {
                        col = col.push(text("In use").size(size::XS).color(HAZE));
                    }
                    tiles.push(pin(col).position(Point::new(x, 0.0)).into());
                }
            };
            push(0, auto_tile(w, h, app.list_focus == 0), pinned.is_none());
            for (i, ch) in c.iter().enumerate() {
                let img = app.thumbs.get(&ch.thumb).map(|p| {
                    image(Handle::from_path(p)).width(w).height(h).content_fit(if app.art_slot == Slot::Logo { ContentFit::Contain } else { ContentFit::Cover })
                });
                push(i + 1, framed(img.map(Into::into), w, h, app.list_focus == i + 1), pinned.as_deref() == Some(ch.url.as_str()));
            }
            iced::widget::Stack::with_children(tiles).width(Fill).height(h + 60.0).into()
        }
    };

    let content = column![Space::new().height(150), head, Space::new().height(48), strip];
    screen(
        app,
        container(content).padding(iced::Padding { left: MARGIN, ..Default::default() }).into(),
        &[(Action::Confirm, "Use this"), (Action::PrevTab, "Switch"), (Action::Back, "Back")],
    )
}

fn auto_tile<'a>(w: f32, h: f32, focused: bool) -> Element<'a, Message> {
    let label = column![
        text("Automatic").size(size::S).font(body(Weight::Bold)),
        text("Best match").size(size::XS).color(HAZE)
    ]
    .spacing(4);
    framed(Some(container(label).padding(20).width(w).height(h).align_y(alignment::Vertical::Bottom).into()), w, h, focused)
}

/// Dark tile with the marquee focus ring.
fn framed<'a>(inner: Option<Element<'a, Message>>, w: f32, h: f32, focused: bool) -> Element<'a, Message> {
    let base = container(inner.unwrap_or_else(|| Space::new().into())).width(w).height(h).clip(true).style(|_| container::Style {
        background: Some(alpha(DUSK, 0.8).into()),
        border: Border { radius: 10.0.into(), ..Default::default() },
        ..Default::default()
    });
    let f = if focused { 1.0 } else { 0.0 };
    let ring = container(Space::new()).width(w).height(h).style(move |_| container::Style {
        background: Some(alpha(NIGHT, 0.3 * (1.0 - f)).into()),
        border: Border { radius: 10.0.into(), width: 3.0 * f, color: alpha(MARQUEE, f) },
        shadow: Shadow { color: alpha(MARQUEE, 0.5 * f), offset: Vector::ZERO, blur_radius: 32.0 * f },
        ..Default::default()
    });
    stack![base, ring].into()
}

/// Shared frame: the game's art behind, a darker wash for the list screens, footer hints.
fn screen<'a>(app: &'a App, content: Element<'a, Message>, hints: &[(Action, &'static str)]) -> Element<'a, Message> {
    let mut r = row![].spacing(36).align_y(alignment::Vertical::Center);
    for &(a, label) in hints {
        r = r.push(row![glyph(app.pad.glyph(a)), text(label).size(size::XS).color(alpha(PAPER, 0.8))].spacing(12).align_y(alignment::Vertical::Center));
    }
    let footer = container(r).width(Fill).height(FOOTER_H).padding([0.0, MARGIN]).align_y(alignment::Vertical::Center);
    let wash = if app.screen == crate::Screen::Game { 0.0 } else { 0.6 };
    let dim = container(Space::new()).width(Fill).height(Fill).style(move |_| container::Style {
        background: Some(Background::Color(alpha(NIGHT, wash))),
        ..Default::default()
    });
    container(stack![art_layers(app), dim, column![container(content).height(Fill), footer], top_bar(app)])
        .width(Fill)
        .height(Fill)
        .style(|_| container::Style { background: Some(NIGHT.into()), text_color: Some(PAPER), ..Default::default() })
        .into()
}
