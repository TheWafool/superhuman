//! System menu (Guide button): a panel on the left over whatever is showing.
//! The running game first, then the library, then power. Groups are set apart by space alone.

use iced::border::Border;
use iced::font::Weight;
use iced::widget::{Space, column, container, image, row, stack, text};
use iced::{Element, Fill, Shadow, Vector};

use super::home::hint;
use crate::input::Action;
use crate::theme::{DUSK, HAZE, MARQUEE, NIGHT, PAPER, alpha, body, display, size};
use crate::{App, MenuItem, Message};

const PANEL_W: f32 = 560.0;
const PAD: f32 = 56.0;
/// One handle for the app's lifetime, so the image is decoded and uploaded once.
static LOGO: std::sync::LazyLock<image::Handle> =
    std::sync::LazyLock::new(|| image::Handle::from_bytes(&include_bytes!("../../dist/superhuman-mark.png")[..]));

fn group(m: MenuItem) -> u8 {
    match m {
        MenuItem::Resume | MenuItem::CloseGame => 0,
        MenuItem::Library | MenuItem::Search | MenuItem::Settings => 1,
        _ => 2,
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let focus = app.menu.unwrap_or(0);

    let logo = image(LOGO.clone()).width(47).height(44);
    let clock = text(app.clock_text()).size(size::XL).font(display(Weight::ExtraBold)).line_height(1.0);
    let mut head = column![row![logo, clock].spacing(18).align_y(iced::Alignment::Center)].spacing(10);
    let sub = match &app.session {
        Some(s) => format!("Playing {}", s.title),
        None => app.clock.format("%A %-d %B").to_string(),
    };
    head = head.push(text(sub).size(size::S).color(HAZE));

    let mut list = column![].spacing(8);
    let mut last = None;
    for (i, (item, label)) in app.menu_items().into_iter().enumerate() {
        if last.is_some_and(|g| g != group(item)) {
            list = list.push(Space::new().height(28));
        }
        last = Some(group(item));
        let danger = app.menu_armed.is_some_and(|(m, _)| m == item);
        list = list.push(super::home::clickable(entry(label, i == focus, danger), Message::Point(crate::Target::Menu(i))));
    }

    let hints = row![hint(app, Action::Confirm, "Select"), hint(app, Action::Back, "Close")].spacing(36);

    let panel = container(column![head, Space::new().height(56), list, Space::new().height(Fill), hints].padding(PAD))
        .width(PANEL_W)
        .height(Fill)
        .style(|_| container::Style {
            background: Some(NIGHT.into()),
            shadow: Shadow { color: alpha(NIGHT, 0.8), offset: Vector::new(12.0, 0.0), blur_radius: 60.0 },
            text_color: Some(PAPER),
            ..Default::default()
        });
    let scrim = container(Space::new()).width(Fill).height(Fill).style(|_| container::Style { background: Some(alpha(NIGHT, 0.55).into()), ..Default::default() });
    // Clicking outside the panel closes the menu.
    let scrim = super::home::clickable(scrim, Message::Act(Action::Back));
    stack![scrim, row![panel, Space::new().width(Fill)]].into()
}

fn entry<'a>(label: String, focused: bool, armed: bool) -> Element<'a, Message> {
    let fg = if focused { NIGHT } else { PAPER };
    container(text(label).size(size::S).font(body(if focused { Weight::Bold } else { Weight::Normal })).color(fg))
        .padding([14.0, 24.0])
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(if focused { MARQUEE.into() } else { alpha(DUSK, 0.0).into() }),
            border: Border { radius: 12.0.into(), width: if armed && !focused { 2.0 } else { 0.0 }, color: MARQUEE },
            shadow: Shadow { color: alpha(MARQUEE, if focused { 0.35 } else { 0.0 }), offset: Vector::ZERO, blur_radius: 24.0 },
            ..Default::default()
        })
        .into()
}
