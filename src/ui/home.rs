//! Home: hero for the focused game, shelves underneath.

use iced::border::Border;
use iced::font::Weight;
use iced::gradient::Linear;
use iced::widget::image::{FilterMethod, Handle};
use iced::widget::{Space, column, container, image, pin, row, stack, text};
use iced::{Background, Color, ContentFit, Degrees, Element, Fill, Gradient, Point, Shadow, Shrink, Vector, alignment};

use crate::input::Action;
use crate::theme::{self, DUSK, HAZE, MARGIN, MARQUEE, NIGHT, PAPER, alpha, body, cover, display, size};
use crate::{App, Focus, Message};

/// Vertical distance between shelves.
pub const ROW_H: f32 = LABEL_H + COVER_BAND + 36.0;
const LABEL_H: f32 = 48.0;
const COVER_BAND: f32 = cover::H * (1.0 + cover::FOCUS_GROW);
const HERO_H: f32 = 400.0;
pub(crate) const FOOTER_H: f32 = 96.0;
/// Sharp hero art height (art is ~3.1:1, designed against a 1920-wide canvas).
const HERO_ART_H: f32 = 620.0;
pub(crate) const LOGO_H: f32 = 128.0;
/// The shelf fade ends at the same darkness as the footer, so there's no seam.
const FOOTER_DIM: f32 = 0.8;

pub fn view(app: &App) -> Element<'_, Message> {
    if app.games.get(app.selected).is_none() {
        return empty(app);
    }
    let (ambient, accent) = app.ambient();

    let background = Gradient::Linear(
        Linear::new(Degrees(165.0))
            .add_stop(0.0, theme::lerp(ambient, accent, 0.35))
            .add_stop(0.45, theme::lerp(ambient, NIGHT, 0.45))
            .add_stop(0.8, NIGHT)
            .add_stop(1.0, NIGHT),
    );

    let content = column![
        container(hero(app)).height(HERO_H).width(Fill),
        container(stack![shelves(app), bottom_fade()]).height(Fill).width(Fill).clip(true),
        footer(app),
    ];

    let layers = stack![art_layers(app), content, top_bar(app)];

    container(layers)
        .width(Fill)
        .height(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Gradient(background)),
            text_color: Some(PAPER),
            ..Default::default()
        })
        .into()
}

/// Blurred backdrop + sharp hero art for the focused game, crossfading from the previous one,
/// under scrims that keep text legible.
pub(crate) fn art_layers(app: &App) -> Element<'_, Message> {
    let t = app.fade();
    let mut layers = stack![].width(Fill).height(Fill);
    for (gi, opacity) in [(app.prev_selected, 1.0 - t), (Some(app.selected), t)] {
        if opacity <= 0.01 {
            continue;
        }
        let Some(art) = gi.and_then(|gi| app.games.get(gi)).and_then(|g| app.art.get(&g.id)) else { continue };
        if let Some(b) = &art.backdrop {
            layers = layers.push(
                image(Handle::from_path(b))
                    .width(Fill)
                    .height(Fill)
                    .content_fit(ContentFit::Cover)
                    .filter_method(FilterMethod::Linear)
                    .opacity(opacity),
            );
        }
        if let Some(h) = &art.hero {
            layers = layers.push(
                image(Handle::from_path(h)).width(Fill).height(HERO_ART_H).content_fit(ContentFit::Cover).opacity(opacity),
            );
        }
    }
    // Darken toward the shelves, and behind the logo/text on the left.
    let down = Gradient::Linear(
        Linear::new(Degrees(180.0))
            .add_stop(0.0, alpha(NIGHT, 0.45))
            .add_stop(0.12, alpha(NIGHT, 0.1))
            .add_stop(0.3, alpha(NIGHT, 0.15))
            .add_stop(0.6, alpha(NIGHT, 0.72))
            .add_stop(1.0, alpha(NIGHT, 0.9)),
    );
    let side = Gradient::Linear(
        Linear::new(Degrees(90.0))
            .add_stop(0.0, alpha(NIGHT, 0.7))
            .add_stop(0.4, alpha(NIGHT, 0.25))
            .add_stop(0.7, alpha(NIGHT, 0.0)),
    );
    for g in [down, side] {
        layers = layers.push(container(Space::new()).width(Fill).height(Fill).style(move |_| container::Style {
            background: Some(Background::Gradient(g)),
            ..Default::default()
        }));
    }
    layers.into()
}

pub(crate) fn top_bar(app: &App) -> Element<'_, Message> {
    let pad = match app.pad {
        crate::input::PadKind::Keyboard => "Keyboard",
        crate::input::PadKind::Xbox => "Controller",
        crate::input::PadKind::PlayStation => "PlayStation controller",
        crate::input::PadKind::Nintendo => "Nintendo controller",
        crate::input::PadKind::Deck => "Steam controller",
    };
    container(
        row![
            text(pad).size(size::XS).color(HAZE),
            text(app.clock.format("%H:%M").to_string()).size(size::S).font(body(Weight::Bold)),
        ]
        .spacing(28)
        .align_y(alignment::Vertical::Center),
    )
    .width(Fill)
    .align_x(alignment::Horizontal::Right)
    .padding([44.0, MARGIN])
    .into()
}

fn hero(app: &App) -> Element<'_, Message> {
    let game = &app.games[app.selected];

    let running = app.session.as_ref().is_some_and(|s| s.game_id == game.id);
    let first = if running { "Running now".to_owned() } else { game.last_played_label() };
    let mut meta = row![text(first).size(size::S).font(body(if running { Weight::Bold } else { Weight::Normal })).color(alpha(PAPER, 0.9))].spacing(36);
    if let Some(p) = game.playtime_label() {
        meta = meta.push(text(p).size(size::S).color(HAZE));
    }
    meta = meta.push(text(game.sources_label()).size(size::S).color(HAZE));

    let hero_f = app.hero_focus.get();
    let buttons = row![
        hero_button(if running { "Now playing" } else { "Play" }, matches!(app.focus, Focus::Hero(0)), hero_f),
        hero_button("Details", matches!(app.focus, Focus::Hero(1)), hero_f),
    ]
    .spacing(20);

    // Logo art when we have it; otherwise the title set large in the display face.
    let logo = app.art.get(&game.id).and_then(|a| a.logo.as_ref());
    let title: Element<'_, Message> = match logo {
        Some(path) => container(
            image(Handle::from_path(path)).height(LOGO_H).width(Shrink).content_fit(ContentFit::Contain),
        )
        .max_width(600)
        .height(LOGO_H)
        .align_y(alignment::Vertical::Bottom)
        .into(),
        None => text(&game.title).size(size::XXL).font(display(Weight::ExtraBold)).line_height(1.0).width(1100).into(),
    };

    container(column![title, Space::new().height(18), meta, Space::new().height(28), buttons])
        .height(Fill)
        .align_y(alignment::Vertical::Bottom)
        .padding(iced::Padding { top: 0.0, left: MARGIN, right: MARGIN, bottom: 44.0 })
        .into()
}

pub(crate) fn hero_button(label: &str, focused: bool, hero_f: f32) -> Element<'_, Message> {
    let t = if focused { hero_f } else { 0.0 };
    let bg = theme::lerp(alpha(DUSK, 0.6), MARQUEE, t);
    let fg = theme::lerp(PAPER, NIGHT, t);
    container(text(label).size(size::S).font(body(Weight::Bold)).color(fg))
        .padding([16.0, 40.0])
        .style(move |_| container::Style {
            background: Some(bg.into()),
            border: Border { radius: 12.0.into(), ..Default::default() },
            shadow: Shadow { color: alpha(MARQUEE, 0.45 * t), offset: Vector::ZERO, blur_radius: 28.0 * t },
            ..Default::default()
        })
        .into()
}

fn shelves(app: &App) -> Element<'_, Message> {
    let mut layers = stack![].width(Fill).height(Fill);
    let scroll_y = app.scroll_y.value;

    for (r, shelf) in app.shelves.iter().enumerate() {
        let y = r as f32 * ROW_H - scroll_y;
        if y < -ROW_H || y > 1080.0 {
            continue;
        }
        let row_active = app.focus == Focus::Shelf && r == app.row;
        let label_color = if row_active { PAPER } else { alpha(PAPER, 0.6) };
        layers = layers.push(
            pin(text(shelf.title).size(size::S).font(body(Weight::Bold)).color(label_color))
                .position(Point::new(MARGIN, y + 4.0)),
        );

        let mut x = MARGIN - app.scroll_x[r].value;
        for (c, &gi) in shelf.games.iter().enumerate() {
            let f = app.grow[r][c].get();
            let s = 1.0 + cover::FOCUS_GROW * f;
            let (w, h) = (cover::W * s, cover::H * s);
            if x + w > -cover::W && x < 4000.0 {
                let top = y + LABEL_H + (COVER_BAND - h) / 2.0;
                let dim = if row_active { 0.0 } else { 0.3 };
                layers = layers.push(pin(cover_tile(app, gi, w, h, f, dim)).position(Point::new(x, top)));
            }
            x += w + cover::GAP;
        }
    }
    layers.into()
}

/// Placeholder cover (real art in M3): game colours with the title in the display face.
fn cover_tile(app: &App, game: usize, w: f32, h: f32, focus: f32, dim: f32) -> Element<'_, Message> {
    let g = &app.games[game];
    let (accent, ambient) = (g.accent, g.ambient);
    let fill = Gradient::Linear(
        Linear::new(Degrees(200.0))
            .add_stop(0.0, accent)
            .add_stop(0.65, theme::lerp(accent, ambient, 0.85))
            .add_stop(1.0, theme::lerp(ambient, NIGHT, 0.4)),
    );
    let scrim = alpha(NIGHT, (0.38 * (1.0 - focus)).max(dim * (1.0 - focus)));

    let title = text(&g.title)
        .size(26.0)
        .font(display(Weight::ExtraBold))
        .line_height(1.05)
        .color(PAPER);

    let art: Element<'_, Message> = match app.art.get(&g.id).and_then(|a| a.cover.as_ref()) {
        Some(path) => image(Handle::from_path(path))
            .width(w)
            .height(h)
            .content_fit(ContentFit::Cover)
            .border_radius(cover::RADIUS)
            .into(),
        // Generated fallback: never a stretched icon.
        None => container(title)
            .width(w)
            .height(h)
            .padding(18.0)
            .align_y(alignment::Vertical::Bottom)
            .style(move |_| container::Style {
                background: Some(Background::Gradient(fill)),
                border: Border { radius: cover::RADIUS.into(), ..Default::default() },
                ..Default::default()
            })
            .into(),
    };

    let overlay = container(Space::new()).width(w).height(h).style(move |_| container::Style {
        background: Some(scrim.into()),
        border: Border {
            radius: cover::RADIUS.into(),
            width: 3.0 * focus,
            color: alpha(MARQUEE, focus),
        },
        shadow: Shadow { color: alpha(MARQUEE, 0.5 * focus), offset: Vector::ZERO, blur_radius: 36.0 * focus },
        snap: false,
        ..Default::default()
    });

    stack![art, overlay].into()
}

/// Fades the next shelf into the footer so it reads as "more below", not as clipped.
fn bottom_fade<'a>() -> Element<'a, Message> {
    let fade = Gradient::Linear(
        Linear::new(Degrees(180.0)).add_stop(0.0, alpha(NIGHT, 0.0)).add_stop(1.0, alpha(NIGHT, FOOTER_DIM)),
    );
    container(
        container(Space::new()).width(Fill).height(110).style(move |_| container::Style {
            background: Some(Background::Gradient(fade)),
            ..Default::default()
        }),
    )
    .height(Fill)
    .align_y(alignment::Vertical::Bottom)
    .into()
}

fn footer(app: &App) -> Element<'_, Message> {
    let hints: &[(Action, &str)] = match app.focus {
        Focus::Hero(_) => &[(Action::Confirm, "Select"), (Action::Back, "Back to shelves")],
        Focus::Shelf => &[(Action::Confirm, "Play"), (Action::Details, "Details"), (Action::Search, "Search"), (Action::Menu, "Menu")],
    };
    let mut r = row![].spacing(36).align_y(alignment::Vertical::Center);
    for &(a, label) in hints {
        r = r.push(
            row![glyph(app.pad.glyph(a)), text(label).size(size::XS).color(alpha(PAPER, 0.8))]
                .spacing(12)
                .align_y(alignment::Vertical::Center),
        );
    }
    container(r)
        .width(Fill)
        .height(FOOTER_H)
        .padding([0.0, MARGIN])
        .align_y(alignment::Vertical::Center)
        .style(|_| container::Style { background: Some(alpha(NIGHT, FOOTER_DIM).into()), ..Default::default() })
        .into()
}

pub(crate) fn glyph(label: &str) -> Element<'_, Message> {
    let wide = label.chars().count() > 1;
    container(text(label).size(15).font(body(Weight::Bold)).color(PAPER))
        .width(if wide { 46.0 } else { 32.0 })
        .height(32)
        .center_x(if wide { 46.0 } else { 32.0 })
        .center_y(32)
        .style(|_| container::Style {
            background: Some(alpha(PAPER, 0.12).into()),
            border: Border { radius: 16.0.into(), width: 1.0, color: alpha(PAPER, 0.35) },
            ..Default::default()
        })
        .into()
}

pub(crate) fn toast(msg: &str) -> Element<'_, Message> {
    container(
        container(text(msg).size(size::S))
            .padding([18.0, 28.0])
            .style(|_| container::Style {
                background: Some(alpha(DUSK, 0.96).into()),
                border: Border { radius: 14.0.into(), width: 1.0, color: alpha(PAPER, 0.12) },
                shadow: Shadow { color: Color::from_rgba(0.0, 0.0, 0.0, 0.35), offset: Vector::new(0.0, 8.0), blur_radius: 30.0 },
                ..Default::default()
            }),
    )
    .width(Fill)
    .height(Fill)
    .align_x(alignment::Horizontal::Center)
    .align_y(alignment::Vertical::Bottom)
    .padding(iced::Padding { bottom: FOOTER_H + 24.0, ..Default::default() })
    .into()
}

/// No games yet: say what to do, not how it feels.
fn empty(app: &App) -> Element<'_, Message> {
    let msg = if app.scanning() { "Looking for games" } else { "No games found yet" };
    container(
        column![
            text(msg).size(size::XL).font(display(Weight::ExtraBold)),
            text("Install a game in Steam, Heroic or Lutris, or add a Flatpak game. It shows up here on its own.")
                .size(size::S)
                .color(HAZE)
                .width(760),
        ]
        .spacing(20),
    )
    .width(Fill)
    .height(Fill)
    .padding(MARGIN)
    .align_y(alignment::Vertical::Center)
    .style(|_| container::Style { background: Some(NIGHT.into()), text_color: Some(PAPER), ..Default::default() })
    .into()
}
