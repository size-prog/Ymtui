use crate::{
    app::{App, CATEGORIES, Focus, Hit},
    model::*,
};
use anyhow::Result;
use ratatui::{
    Frame, Terminal,
    backend::TestBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Wrap},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

const BG: Color = Color::Rgb(14, 17, 22);
const PANEL: Color = Color::Rgb(19, 23, 29);
const BORDER: Color = Color::Rgb(44, 53, 64);
const GREEN: Color = Color::Rgb(102, 226, 155);
const TEXT: Color = Color::Rgb(222, 229, 237);
const MUTED: Color = Color::Rgb(129, 143, 159);
const SELECTED: Color = Color::Rgb(33, 55, 48);
fn style(color: Color) -> Style {
    Style::default().fg(color)
}
fn bold(color: Color) -> Style {
    style(color).add_modifier(Modifier::BOLD)
}
fn block(title: &str, active: bool) -> Block<'_> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style(if active { GREEN } else { BORDER }))
        .style(style(TEXT).bg(PANEL))
}
fn text(f: &mut Frame, rect: Rect, s: impl Into<String>, color: Color) {
    f.render_widget(Paragraph::new(s.into()).style(style(color)), rect);
}
fn line_rect(r: Rect, y: u16) -> Rect {
    Rect::new(
        r.x,
        r.y.saturating_add(y),
        r.width,
        if y < r.height { 1 } else { 0 },
    )
}
fn hit(app: &mut App, r: Rect, h: Hit) {
    if r.width > 0 && r.height > 0 {
        app.hits.push((r, h));
    }
}
pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.hits.clear();
    f.render_widget(Block::default().style(style(TEXT).bg(BG)), area);
    if area.width < 48 || area.height < 14 {
        app.queue_visible = false;
        if app.focus == Focus::Queue {
            app.focus = Focus::Content;
        }
        f.render_widget(
            Paragraph::new(
                "YaMusic\n\nУвеличьте терминал до 48 × 14\nДля трёх панелей: 110 × 28\n\nq — выход",
            )
            .alignment(Alignment::Center)
            .style(style(GREEN)),
            area,
        );
        return;
    }
    let vertical = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(6),
        Constraint::Length(1),
    ])
    .split(area);
    header(f, app, vertical[0]);
    let side = if area.width >= 90 { 24 } else { 18 };
    let show_right = app.show_queue && area.width >= 110;
    app.queue_visible = show_right;
    if !show_right && app.focus == Focus::Queue {
        app.focus = Focus::Content;
    }
    let columns = if show_right {
        Layout::horizontal([
            Constraint::Length(side),
            Constraint::Min(34),
            Constraint::Length(30),
        ])
        .split(vertical[1])
    } else {
        Layout::horizontal([Constraint::Length(side), Constraint::Min(28)]).split(vertical[1])
    };
    navigation(f, app, columns[0]);
    content(f, app, columns[1]);
    if show_right {
        right(f, app, columns[2]);
    }
    player(f, app, vertical[2]);
    let status = if !app.status.is_empty() {
        app.status.clone()
    } else if app.demo {
        "DEMO · без звука   / Поиск   a Исполнитель   A Альбом   Tab Панели   ? Помощь".into()
    } else {
        "/ Поиск   a Исполнитель   A Альбом   e В очередь   Tab Панели   ? Помощь".into()
    };
    text(
        f,
        vertical[3],
        format!(" {status}"),
        if app.status_error {
            Color::Rgb(255, 137, 137)
        } else {
            MUTED
        },
    );
    if app.help {
        help(f, area);
    }
    if let Some(input) = &app.input {
        let query = input.clone();
        input_modal(f, app, area, &query);
    }
}
fn header(f: &mut Frame, app: &mut App, r: Rect) {
    let cols = Layout::horizontal([Constraint::Length(24), Constraint::Min(20)]).split(r);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ◉ ", bold(GREEN)),
            Span::styled("YaMusic", bold(TEXT)),
        ]))
        .block(Block::default().padding(ratatui::widgets::Padding::top(1))),
        cols[0],
    );
    let search = block("", false);
    f.render_widget(
        Paragraph::new("  /  Поиск музыки, исполнителей, альбомов")
            .style(style(MUTED))
            .block(search),
        cols[1],
    );
    hit(app, cols[1], Hit::Search);
}
fn navigation(f: &mut Frame, app: &mut App, r: Rect) {
    hit(app, r, Hit::Panel(Focus::Nav));
    let b = block("Библиотека", app.focus == Focus::Nav);
    let inner = b.inner(r);
    f.render_widget(b, r);
    let names = [
        "⌂  Главная",
        "◉  Моя волна",
        "♥  Мне нравится",
        "≡  Плейлисты",
        "⌕  Поиск",
        "☷  Очередь",
    ];
    for (i, name) in names.iter().enumerate() {
        let row = line_rect(inner, 1 + i as u16 * 2);
        let active = app.nav == i;
        f.render_widget(
            Paragraph::new(format!(" {} {name}", i + 1)).style(if active {
                bold(GREEN).bg(SELECTED)
            } else {
                style(TEXT)
            }),
            row,
        );
        hit(app, row, Hit::Nav(i));
    }
    if inner.height >= 21 {
        text(f, line_rect(inner, 15), " ───────────────────", BORDER);
        text(f, line_rect(inner, 17), " СОЗДАНО ДЛЯ ВАС", MUTED);
        text(f, line_rect(inner, 19), " Откройте свою волну", TEXT);
        text(f, line_rect(inner, 20), " и найдите новый звук", MUTED);
    }
    if inner.height > 25 {
        text(
            f,
            line_rect(inner, inner.height - 3),
            " ?  Горячие клавиши",
            MUTED,
        );
        hit(app, line_rect(inner, inner.height - 3), Hit::Help);
    }
}
fn content(f: &mut Frame, app: &mut App, r: Rect) {
    hit(app, r, Hit::Panel(Focus::Content));
    let b = block("Обзор", app.focus == Focus::Content);
    let inner = b.inner(r);
    f.render_widget(b, r);
    if inner.height < 5 {
        return;
    }
    let nav = Rect::new(inner.x + 1, inner.y, 3, 1);
    text(f, nav, " ‹ ", TEXT);
    hit(app, nav, Hit::Back);
    let nav = Rect::new(inner.x + 5, inner.y, 3, 1);
    text(f, nav, " › ", TEXT);
    hit(app, nav, Hit::Forward);
    let count = app.visible().len();
    text(
        f,
        Rect::new(inner.x + 10, inner.y, inner.width.saturating_sub(11), 1),
        format!("ВАША КОЛЛЕКЦИЯ  /  {count}"),
        MUTED,
    );
    f.render_widget(
        Paragraph::new(app.page.title.clone()).style(bold(GREEN)),
        Rect::new(inner.x + 1, inner.y + 2, inner.width.saturating_sub(2), 1),
    );
    let description_h = if matches!(app.page.route, Route::Artist(_)) {
        3
    } else {
        2
    };
    f.render_widget(
        Paragraph::new(app.page.description.clone())
            .style(style(MUTED))
            .wrap(Wrap { trim: true }),
        Rect::new(
            inner.x + 1,
            inner.y + 3,
            inner.width.saturating_sub(2),
            description_h.min(inner.height.saturating_sub(3)),
        ),
    );
    let mut y = inner.y + 3 + description_h + 1;
    if app.page.route == Route::Radio {
        let mut x = inner.x;
        for (i, c) in CATEGORIES.iter().enumerate() {
            let width = (c.chars().count() + 2) as u16;
            if x + width > inner.right() {
                break;
            }
            let rect = Rect::new(x, y, width, 1);
            f.render_widget(
                Paragraph::new(format!(" {c} ")).style(if app.category == i {
                    bold(BG).bg(GREEN)
                } else {
                    style(MUTED)
                }),
                rect,
            );
            hit(app, rect, Hit::Category(i));
            x += width + 1;
        }
        y += 2;
    }
    let bottom = inner.bottom().saturating_sub(1);
    if y >= bottom {
        return;
    }
    if app.loading {
        text(
            f,
            Rect::new(inner.x + 2, y, inner.width.saturating_sub(4), 1),
            "◌  Загружаем вашу музыку…",
            GREEN,
        );
        return;
    }
    let visible = app.visible();
    if visible.is_empty() {
        f.render_widget(
            Paragraph::new(if !app.filter.is_empty() {
                "Ничего не найдено\n/ — изменить фильтр"
            } else {
                "Здесь пока тихо\n/ — найти музыку · 2 — запустить волну"
            })
            .style(style(MUTED)),
            Rect::new(inner.x + 2, y, inner.width.saturating_sub(4), 3),
        );
        return;
    }
    let columns = track_columns(inner.width);
    draw_cells(
        f,
        Rect::new(inner.x, y, inner.width, 1),
        &columns,
        &[
            "#".into(),
            "НАЗВАНИЕ".into(),
            "ИСПОЛНИТЕЛЬ".into(),
            "АЛЬБОМ".into(),
            "ВРЕМЯ".into(),
        ],
        style(MUTED),
    );
    y += 1;
    let height = bottom.saturating_sub(y) as usize;
    app.page.selected = app.page.selected.min(visible.len() - 1);
    if app.page.selected < app.page.offset {
        app.page.offset = app.page.selected;
    }
    if app.page.selected >= app.page.offset + height {
        app.page.offset = app.page.selected.saturating_sub(height.saturating_sub(1));
    }
    for (row, (position, index)) in visible
        .iter()
        .enumerate()
        .skip(app.page.offset)
        .take(height)
        .enumerate()
    {
        let item = &app.page.items[*index];
        let chosen = position == app.page.selected;
        let current =
            matches!(item,Item::Track(t)if app.queue.current().map(|c|c.id==t.id).unwrap_or(false));
        let number = if current && app.audio.playing {
            if app.audio.paused {
                "Ⅱ".into()
            } else {
                "▶".into()
            }
        } else {
            format!("{}", position + 1)
        };
        let cells = match item {
            Item::Track(t) => vec![
                number,
                format!(
                    "{}{}",
                    if app.liked.contains(&t.id) {
                        "♥ "
                    } else {
                        ""
                    },
                    t.title
                ),
                t.artist(),
                t.album.title.clone(),
                time(t.duration()),
            ],
            _ => vec![
                number,
                item.title(),
                item.subtitle(),
                String::new(),
                "→".into(),
            ],
        };
        let rect = Rect::new(inner.x, y + row as u16, inner.width, 1);
        let s = if chosen {
            bold(TEXT).bg(SELECTED)
        } else if current {
            style(GREEN)
        } else {
            style(TEXT)
        };
        f.render_widget(Block::default().style(s), rect);
        draw_cells(f, rect, &columns, &cells, s);
        let is_track = matches!(item, Item::Track(_));
        hit(app, rect, Hit::Row(position));
        if is_track {
            let cells = Layout::horizontal(columns.clone()).split(rect);
            hit(app, cells[2], Hit::RowArtist(position));
            hit(app, cells[3], Hit::RowAlbum(position));
        }
    }
    let footer = format!(
        " {} / {}   Enter Открыть · a Артист · e В очередь",
        app.page.selected + 1,
        visible.len()
    );
    text(f, Rect::new(inner.x, bottom, inner.width, 1), footer, MUTED);
}
fn track_columns(width: u16) -> Vec<Constraint> {
    if width >= 76 {
        vec![
            Constraint::Length(4),
            Constraint::Percentage(32),
            Constraint::Percentage(24),
            Constraint::Min(12),
            Constraint::Length(6),
        ]
    } else if width >= 48 {
        vec![
            Constraint::Length(3),
            Constraint::Percentage(46),
            Constraint::Min(10),
            Constraint::Length(0),
            Constraint::Length(6),
        ]
    } else {
        vec![
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(0),
            Constraint::Length(0),
            Constraint::Length(5),
        ]
    }
}
fn draw_cells(f: &mut Frame, r: Rect, constraints: &[Constraint], cells: &[String], s: Style) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(r);
    for (i, c) in cells.iter().enumerate() {
        f.render_widget(Paragraph::new(c.as_str()).style(s), cols[i]);
    }
}
fn right(f: &mut Frame, app: &mut App, r: Rect) {
    let b = block("Далее в очереди", app.focus == Focus::Queue);
    hit(app, r, Hit::Panel(Focus::Queue));
    let inner = b.inner(r);
    f.render_widget(b, r);
    if inner.height == 0 {
        return;
    }
    let height = inner.height.saturating_sub(2) as usize;
    if app.queue_selected < app.queue_offset {
        app.queue_offset = app.queue_selected;
    }
    if app.queue_selected >= app.queue_offset + height {
        app.queue_offset = app.queue_selected.saturating_sub(height.saturating_sub(1));
    }
    for (row, (i, t)) in app
        .queue
        .tracks
        .iter()
        .enumerate()
        .skip(app.queue_offset)
        .take(height)
        .enumerate()
    {
        let current = app.queue.index == Some(i);
        let rect = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
        f.render_widget(
            Paragraph::new(format!(" {} {}", if current { "▶" } else { "·" }, t.title)).style(
                if i == app.queue_selected && app.focus == Focus::Queue {
                    bold(TEXT).bg(SELECTED)
                } else {
                    style(if current { GREEN } else { TEXT })
                },
            ),
            rect,
        );
        app.hits.push((rect, Hit::Queue(i)));
    }
    if app.queue.tracks.is_empty() {
        text(f, line_rect(inner, 1), " e — добавить трек", MUTED);
    }
    if inner.height >= 2 {
        text(
            f,
            line_rect(inner, inner.height - 1),
            format!(" {} треков · J/K порядок", app.queue.tracks.len()),
            MUTED,
        );
    }
}
fn player(f: &mut Frame, app: &mut App, r: Rect) {
    let b = block(app.radio_name.as_deref().unwrap_or("Плеер"), false);
    let inner = b.inner(r);
    f.render_widget(b, r);
    let t = app.queue.current();
    let title = t
        .map(|t| format!("{} — {}", t.title, t.artist()))
        .unwrap_or_else(|| "Начните с любимой музыки".into());
    text(
        f,
        line_rect(inner, 0),
        format!(" {}{}", if app.resolving { "◌ " } else { "" }, title),
        TEXT,
    );
    let duration = t.map(Track::duration).unwrap_or(0.0);
    let position = app.audio.position();
    let ratio = if duration > 0.0 {
        (position / duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let progress = Layout::horizontal([
        Constraint::Length(6),
        Constraint::Min(4),
        Constraint::Length(7),
    ])
    .split(line_rect(inner, 1));
    text(f, progress[0], format!(" {}", time(position)), MUTED);
    f.render_widget(
        Gauge::default()
            .ratio(ratio)
            .label("")
            .gauge_style(style(GREEN).bg(BORDER))
            .use_unicode(true),
        progress[1],
    );
    hit(app, progress[1], Hit::Seek);
    text(f, progress[2], format!(" {}", time(duration)), MUTED);
    let controls = [
        ("  |◀  ".to_string(), Hit::Previous),
        (
            if app.audio.playing && !app.audio.paused {
                "  Ⅱ  "
            } else {
                "  ▶  "
            }
            .into(),
            Hit::Pause,
        ),
        ("  ▶|  ".into(), Hit::Next),
        (
            format!(" Повтор:{} ", app.queue.repeat.label()),
            Hit::Repeat,
        ),
        (
            format!(
                " Случайно:{} ",
                if app.queue.shuffle { "да" } else { "нет" }
            ),
            Hit::Shuffle,
        ),
        (
            format!(" Громкость:{:3.0}% ", app.audio.volume),
            Hit::Volume,
        ),
        (" Очередь ".into(), Hit::QueueToggle),
        (" ? ".into(), Hit::Help),
    ];
    let mut x = inner.x;
    for (label, h) in controls {
        let width = label.chars().count() as u16;
        if x + width > inner.right() {
            break;
        }
        let rect = Rect::new(x, inner.y + 3, width, 1);
        f.render_widget(
            Paragraph::new(label).style(if matches!(h, Hit::Pause) {
                bold(GREEN)
            } else {
                style(MUTED)
            }),
            rect,
        );
        hit(app, rect, h);
        x += width;
    }
}
fn modal(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(4));
    let h = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
fn help(f: &mut Frame, area: Rect) {
    let r = modal(area, 80, 28);
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(vec![
            Line::styled("Музыка под рукой", bold(GREEN)),
            Line::from(""),
            Line::from("1 Главная   2 Волна   3 Любимое   4 Плейлисты   5 Поиск   6 Очередь"),
            Line::from(""),
            Line::from("Tab / Shift+Tab   Переключить панель"),
            Line::from("j/k или ↑/↓       Выбрать строку · PgUp/PgDn — страница"),
            Line::from("Enter             Открыть / воспроизвести · двойной клик мышью"),
            Line::from("/                 Поиск · на странице радио — локальный фильтр"),
            Line::from("Ctrl+F            Глобальный поиск с любого экрана"),
            Line::from("a / A             Исполнитель / альбом выбранного трека"),
            Line::from("w                 Радио исполнителя"),
            Line::from("Alt+← / Alt+→      Назад / вперёд · Esc — назад"),
            Line::from(""),
            Line::from("Пробел            Пауза / продолжить"),
            Line::from("x                 Остановить, отменить загрузку потока и радио"),
            Line::from("n / p             Следующий / предыдущий трек"),
            Line::from("← / →             Перемотать на 5 секунд · клик по шкале — перейти"),
            Line::from("+ / -             Громкость"),
            Line::from("l                 Добавить / удалить из любимого"),
            Line::from("e                 Добавить в очередь"),
            Line::from("Delete / J / K    Удалить / переместить в очереди"),
            Line::from("r / s / Q         Повтор / случайный порядок / панель очереди"),
            Line::from("[ / ] (х / ъ)     Категория радио"),
            Line::from("F5                Обновить страницу"),
            Line::from("? / F1 / Esc      Закрыть справку · q — выход из приложения"),
        ])
        .block(block("Горячие клавиши", true))
        .style(style(TEXT)),
        r,
    );
}
fn input_modal(f: &mut Frame, app: &App, area: Rect, query: &str) {
    let r = modal(area, 76, 7);
    f.render_widget(Clear, r);
    let b = block(
        if !app.input_global {
            "Фильтр станций"
        } else {
            "Поиск по Яндекс Музыке"
        },
        true,
    );
    let inner = b.inner(r);
    f.render_widget(b, r);
    text(
        f,
        line_rect(inner, 0),
        " Название трека, имя исполнителя или альбом",
        MUTED,
    );
    let cursor = app.input_cursor.min(query.len());
    let cursor = if query.is_char_boundary(cursor) {
        cursor
    } else {
        query.len()
    };
    let column = unicode_width::UnicodeWidthStr::width(&query[..cursor]) + 3;
    let scroll = column
        .saturating_sub(inner.width.saturating_sub(2) as usize)
        .min(u16::MAX as usize) as u16;
    f.render_widget(
        Paragraph::new(format!(" › {}▏{}", &query[..cursor], &query[cursor..]))
            .scroll((0, scroll))
            .style(bold(TEXT)),
        line_rect(inner, 2),
    );
    text(
        f,
        line_rect(inner, 4),
        " Enter — найти   Esc — закрыть   Ctrl+U — очистить",
        MUTED,
    );
}
pub fn snapshot(app: &mut App, path: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.loading && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        app.pump();
    }
    let backend = TestBackend::new(132, 38);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| draw(f, app))?;
    let buf = terminal.backend().buffer();
    let mut result = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            result.push_str(buf[(x, y)].symbol());
        }
        result.push('\n');
    }
    std::fs::write(path, result)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renders_all_terminal_sizes() {
        let mut app = App::new(None, true, true).unwrap();
        app.page = crate::api::Backend::new(None, true)
            .unwrap()
            .page(Route::Home)
            .unwrap();
        app.loading = false;
        for (w, h) in [(132, 38), (80, 24), (48, 14), (30, 8)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| draw(f, &mut app)).unwrap();
        }
    }
    #[test]
    fn artist_page_and_modal_render() {
        let mut app = App::new(None, true, true).unwrap();
        app.page = crate::api::Backend::new(None, true)
            .unwrap()
            .page(Route::Artist("aster-city".into()))
            .unwrap();
        app.loading = false;
        let mut t = Terminal::new(TestBackend::new(132, 38)).unwrap();
        t.draw(|f| draw(f, &mut app)).unwrap();
        let text = format!("{:?}", t.backend().buffer());
        assert!(text.contains("Aster City"));
        app.help = true;
        t.draw(|f| draw(f, &mut app)).unwrap();
        app.help = false;
        app.input = Some("Ночная музыка".into());
        t.draw(|f| draw(f, &mut app)).unwrap();
    }
}
