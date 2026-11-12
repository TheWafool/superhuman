//! Settings and the hidden-games list.

use iced::border::Border;
use iced::font::Weight;
use iced::widget::{Space, column, container, row, text};
use iced::{Element, Fill, Shadow, Vector, alignment};

use super::game::screen;
use crate::input::Action;
use crate::theme::{DUSK, HAZE, MARGIN, MARQUEE, NIGHT, PAPER, alpha, body, display, size};
use crate::{App, Message};

pub fn view(app: &App) -> Element<'_, Message> {
    let mut list = column![].spacing(10);
    for (i, (_, label, value)) in app.settings_rows().into_iter().enumerate() {
        list = list.push(setting(label, value, i == app.list_focus));
    }
    let content = column![Space::new().height(150), text("Settings").size(size::XL).font(display(Weight::ExtraBold)), Space::new().height(40), list].padding([0.0, MARGIN]);
    screen(app, content.into(), &[(Action::Confirm, "Change"), (Action::Back, "Back")])
}

fn setting<'a>(label: &'a str, value: String, focused: bool) -> Element<'a, Message> {
    let fg = if focused { NIGHT } else { PAPER };
    let sub = if focused { alpha(NIGHT, 0.7) } else { HAZE };
    container(
        row![text(label).size(size::S).font(body(Weight::Bold)).color(fg), Space::new().width(Fill), text(value).size(size::S).color(sub)]
            .align_y(alignment::Vertical::Center),
    )
    .padding([18.0, 28.0])
    .width(1100)
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
        list = list.push(setting(&r.title, format!("via {}", r.source.label()), i == app.list_focus));
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
