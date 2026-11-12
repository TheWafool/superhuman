//! "Now playing": covers the library while a game starts and runs.
//! The game's art fills the screen, the logo sits centred, and one line says what's happening.

use iced::font::Weight;
use iced::gradient::Linear;
use iced::widget::image::Handle;
use iced::widget::{Space, column, container, image, row, stack, text};
use iced::{Background, ContentFit, Degrees, Element, Fill, Gradient, Shrink, alignment};

use crate::input::Action;
use crate::launch::Status;
use crate::theme::{self, MARGIN, NIGHT, PAPER, alpha, body, display, size};
use crate::{App, Message};

/// "Starting…" only appears if the game hasn't shown up by then (most do sooner).
const SLOW_START_MS: u128 = 1500;

pub fn view(app: &App) -> Element<'_, Message> {
    let t = app.playing.get();
    let Some(sess) = &app.session else {
        // Fading out after the game closed: just the curtain.
        return curtain(t);
    };
    let game = app.games.iter().find(|g| g.id == sess.game_id);
    let art = game.and_then(|g| app.art.get(&g.id));
    let accent = game.map_or(theme::DUSK, |g| g.accent);

    let mut layers = stack![curtain(t)].width(Fill).height(Fill);
    if let Some(h) = art.and_then(|a| a.backdrop.as_ref()) {
        layers = layers.push(image(Handle::from_path(h)).width(Fill).height(Fill).content_fit(ContentFit::Cover).opacity(0.9 * t));
    }
    if let Some(h) = art.and_then(|a| a.hero.as_ref()) {
        layers = layers.push(image(Handle::from_path(h)).width(Fill).height(Fill).content_fit(ContentFit::Cover).opacity(0.55 * t));
    }
    // Deepen toward the bottom, tinted with the game's own accent.
    let scrim = Gradient::Linear(
        Linear::new(Degrees(180.0))
            .add_stop(0.0, alpha(NIGHT, 0.35 * t))
            .add_stop(0.55, alpha(theme::lerp(NIGHT, accent, 0.12), 0.6 * t))
            .add_stop(1.0, alpha(NIGHT, 0.95 * t)),
    );
    layers = layers.push(container(Space::new()).width(Fill).height(Fill).style(move |_| container::Style {
        background: Some(Background::Gradient(scrim)),
        ..Default::default()
    }));

    // The logo grows a little as the curtain comes down: the one moment of motion here.
    let logo_h = 150.0 + 40.0 * t;
    let title: Element<'_, Message> = match art.and_then(|a| a.logo.as_ref()) {
        Some(p) => container(image(Handle::from_path(p)).height(logo_h).width(Shrink).content_fit(ContentFit::Contain).opacity(t))
            .max_width(900)
            .into(),
        None => text(&sess.title)
            .size(size::XXL)
            .font(display(Weight::ExtraBold))
            .line_height(1.0)
            .align_x(alignment::Horizontal::Center)
            .color(alpha(PAPER, t))
            .width(1200)
            .into(),
    };

    let launched_ms = sess.launched.elapsed().as_millis();
    let status = match app.status {
        Status::Running => sess.running_since.map(|s| playing_for(s.elapsed().as_secs() / 60)).unwrap_or_default(),
        Status::Starting if launched_ms > 20_000 => "Still starting. The launcher may be updating or syncing.".into(),
        Status::Starting if launched_ms > SLOW_START_MS => "Starting…".into(),
        _ => String::new(),
    };

    let centre = column![title, text(status).size(size::M).color(alpha(PAPER, 0.9 * t))]
        .spacing(36)
        .align_x(alignment::Horizontal::Center);

    let close_label = if app.stop_armed.is_some() { "Press again to close the game" } else { "Close game" };
    let hints = row![hint(app, Action::Back, "Back to library", t), hint(app, Action::Details, close_label, t)]
        .spacing(36)
        .align_y(alignment::Vertical::Center);

    let content = column![
        container(centre).width(Fill).height(Fill).center_x(Fill).center_y(Fill),
        container(hints).width(Fill).padding([40.0, MARGIN]),
    ];
    layers.push(content).into()
}

fn playing_for(mins: u64) -> String {
    match mins {
        0 => "Playing now".into(),
        1 => "Playing for 1 minute".into(),
        m if m < 120 => format!("Playing for {m} minutes"),
        m => format!("Playing for {} hours {} minutes", m / 60, m % 60),
    }
}

fn curtain<'a>(t: f32) -> Element<'a, Message> {
    container(Space::new())
        .width(Fill)
        .height(Fill)
        .style(move |_| container::Style { background: Some(alpha(NIGHT, t).into()), ..Default::default() })
        .into()
}

fn hint<'a>(app: &App, a: Action, label: &'a str, t: f32) -> Element<'a, Message> {
    let g = app.glyph(a);
    let r = row![
        container(text(g).size(15).font(body(Weight::Bold)).color(alpha(PAPER, t)))
            .width(if g.chars().count() > 1 { 46.0 } else { 32.0 })
            .height(32)
            .center_x(if g.chars().count() > 1 { 46.0 } else { 32.0 })
            .center_y(32)
            .style(move |_| container::Style {
                background: Some(alpha(PAPER, 0.12 * t).into()),
                border: iced::border::Border { radius: 16.0.into(), width: 1.0, color: alpha(PAPER, 0.35 * t) },
                ..Default::default()
            }),
        text(label).size(size::XS).color(alpha(PAPER, 0.8 * t)),
    ]
    .spacing(12)
    .align_y(alignment::Vertical::Center);
    super::home::clickable(r, Message::Act(a))
}
