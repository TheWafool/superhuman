//! Settings and the hidden-games list.

use iced::border::Border;
use iced::font::Weight;
use iced::widget::{Space, column, container, row, text};
use iced::{Element, Fill, Shadow, Vector, alignment};

use super::game::screen;
use crate::input::Action;
use crate::theme::{DUSK, HAZE, MARGIN, MARQUEE, NIGHT, PAPER, alpha, body, display, size};
use super::home::clickable;
use crate::{App, Message, SettingInfo};

/// Most entries (rows plus section headings) on screen at once; the list scrolls with focus.
const VISIBLE: usize = 11;

pub fn view(app: &App) -> Element<'_, Message> {
    let rows = app.settings_rows();
    let focus = app.list_focus.min(rows.len() - 1);
    // Flatten into headings and rows, remembering where the focused row lands.
    let mut entries: Vec<Element<'_, Message>> = Vec::new();
    let mut focus_at = 0;
    let mut section = "";
    for (i, SettingInfo(_, sec, label, value, _)) in rows.iter().enumerate() {
        if *sec != section {
            section = sec;
            entries.push(
                container(text(*sec).size(size::XS).font(body(Weight::Bold)).color(HAZE)).padding(iced::Padding { top: 18.0, bottom: 4.0, left: 4.0, right: 0.0 }).into(),
            );
        }
        if i == focus {
            focus_at = entries.len();
        }
        entries.push(clickable(setting(label, value.clone(), i == focus), Message::Point(crate::Target::Row(i))));
    }
    let start = focus_at.saturating_sub(VISIBLE / 2).min(entries.len().saturating_sub(VISIBLE));
    let list = column(entries.into_iter().skip(start).take(VISIBLE)).spacing(8).width(900);

    let help = column![
        text(rows[focus].2).size(size::M).font(display(Weight::ExtraBold)),
        Space::new().height(16),
        text(rows[focus].4).size(size::S).color(alpha(PAPER, 0.8)).line_height(1.4),
    ]
    .width(520);
    let content = column![
        Space::new().height(120),
        text("Settings").size(size::XL).font(display(Weight::ExtraBold)),
        Space::new().height(24),
        row![list, Space::new().width(80), help],
    ]
    .padding([0.0, MARGIN]);
    let change = if rows[focus].0 == crate::SettingRow::Sources || !matches!(rows[focus].3.as_str(), "On" | "Off") { "Select" } else { "Change" };
    screen(app, content.into(), &[(Action::Confirm, change), (Action::Back, "Back")])
}

pub fn sources(app: &App) -> Element<'_, Message> {
    let rows = app.source_rows();
    let focus = app.list_focus.min(rows.len().saturating_sub(1));
    let start = focus.saturating_sub(4).min(rows.len().saturating_sub(9));
    let mut list = column![].spacing(8).width(900);
    for (i, (src, on)) in rows.iter().enumerate().skip(start).take(9) {
        list = list.push(clickable(setting(src.label(), if *on { "On".into() } else { "Off".into() }, i == focus), Message::Point(crate::Target::Row(i))));
    }
    let content = column![
        Space::new().height(120),
        text("Game sources").size(size::XL).font(display(Weight::ExtraBold)),
        Space::new().height(14),
        text("Turned-off sources are left out of your library. Nothing is uninstalled.").size(size::S).color(HAZE),
        Space::new().height(32),
        list
    ]
    .padding([0.0, MARGIN]);
    screen(app, content.into(), &[(Action::Confirm, "Turn on or off"), (Action::Back, "Back")])
}

fn setting<'a>(label: &'a str, value: String, focused: bool) -> Element<'a, Message> {
    let fg = if focused { NIGHT } else { PAPER };
    let sub = if focused { alpha(NIGHT, 0.7) } else { HAZE };
    container(
        row![text(label).size(size::S).font(body(Weight::Bold)).color(fg), Space::new().width(Fill), text(value).size(size::S).color(sub)]
            .align_y(alignment::Vertical::Center),
    )
    .padding([18.0, 28.0])
    .width(Fill)
    .style(move |_| container::Style {
        background: Some(if focused { MARQUEE.into() } else { alpha(DUSK, 0.55).into() }),
        border: Border { radius: 12.0.into(), ..Default::default() },
        shadow: Shadow { color: alpha(MARQUEE, if focused { 0.4 } else { 0.0 }), offset: Vector::ZERO, blur_radius: 24.0 },
        ..Default::default()
    })
    .into()
}

pub fn hidden(app: &App) -> Element<'_, Message> {
    let mut list = column![].spacing(10);
    if app.hidden.is_empty() {
        list = list.push(text("No hidden games.").size(size::S).color(HAZE));
    }
    let start = app.list_focus.saturating_sub(4).min(app.hidden.len().saturating_sub(9));
    for (i, r) in app.hidden.iter().enumerate().skip(start).take(9) {
        list = list.push(super::home::clickable(setting(&r.title, format!("via {}", r.source.label()), i == app.list_focus), Message::Point(crate::Target::Row(i))));
    }
    let content = column![
        Space::new().height(150),
        text("Hidden games").size(size::XL).font(display(Weight::ExtraBold)),
        Space::new().height(14),
        text("Pick a game to put it back in your library.").size(size::S).color(HAZE),
        Space::new().height(40),
        list
    ]
    .padding([0.0, MARGIN]);
    screen(app, content.into(), &[(Action::Confirm, "Unhide"), (Action::Back, "Back")])
}
