use crate::{
    api::{Backend, Wave},
    audio::Audio,
    config::Preferences,
    media::Media,
    model::*,
};
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MediaKeyCode, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};
use std::{
    collections::{HashMap, HashSet},
    io,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

// Translate command keys by their position on the standard Russian ЙЦУКЕН
// keyboard. Text input must keep the original characters.
fn shortcut_code(code: KeyCode) -> KeyCode {
    let KeyCode::Char(c) = code else {
        return code;
    };
    const RU: &str = "ёйцукенгшщзхъфывапролджэячсмитьбю.ЁЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ,";
    const EN: &str = "`qwertyuiop[]asdfghjkl;'zxcvbnm,./~QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>?";
    KeyCode::Char(
        RU.chars()
            .zip(EN.chars())
            .find_map(|(ru, en)| (c == ru).then_some(en))
            .unwrap_or(c),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Nav,
    Content,
    Queue,
}
#[derive(Clone, Debug)]
pub enum Hit {
    Panel(Focus),
    Nav(usize),
    Row(usize),
    RowArtist(usize),
    RowAlbum(usize),
    Queue(usize),
    Seek,
    Previous,
    Pause,
    Next,
    Volume,
    Repeat,
    Shuffle,
    Help,
    Search,
    QueueToggle,
    Back,
    Forward,
    Category(usize),
}
enum Reply {
    Page(u64, Result<Page>),
    Stream(u64, Track, Result<String>),
    Wave(u64, Station, bool, Result<Wave>),
    Like(String, bool, Result<()>),
    Feedback(Result<()>),
    LikedIds(Result<HashSet<String>>),
}
pub const CATEGORIES: [&str; 5] = ["Все", "Жанры", "Настроения", "Занятия", "Другое"];
#[derive(Clone)]
struct View {
    page: Page,
    filter: String,
    category: usize,
    nav: usize,
    loading: bool,
}
pub struct App {
    backend: Arc<Backend>,
    tx: Sender<Reply>,
    rx: Receiver<Reply>,
    pub demo: bool,
    pub page: Page,
    pub queue: PlaybackQueue,
    pub audio: Audio,
    pub focus: Focus,
    pub nav: usize,
    pub queue_selected: usize,
    pub queue_offset: usize,
    pub show_queue: bool,
    pub queue_visible: bool,
    pub help: bool,
    pub input: Option<String>,
    pub input_global: bool,
    pub input_cursor: usize,
    pub filter: String,
    pub category: usize,
    pub loading: bool,
    pub resolving: bool,
    pub status: String,
    pub status_error: bool,
    pub quit: bool,
    pub hits: Vec<(Rect, Hit)>,
    pub liked: HashSet<String>,
    pending_likes: HashSet<String>,
    like_overrides: HashMap<String, bool>,
    likes_ready: bool,
    back: Vec<View>,
    forward: Vec<View>,
    page_generation: u64,
    play_generation: u64,
    pause_pending: bool,
    radio_generation: u64,
    radio: Option<(Station, String)>,
    radio_pending: bool,
    radio_retry: Instant,
    batches: HashMap<String, String>,
    pub radio_name: Option<String>,
    radio_waiting: bool,
    media: Option<Media>,
    media_track: String,
    last_media: Instant,
    last_click: Option<(u16, u16, Instant)>,
    status_since: Instant,
}
impl App {
    pub fn new(token: Option<&str>, demo: bool, no_media: bool) -> Result<Self> {
        let backend = Arc::new(Backend::new(token, demo)?);
        let (tx, rx) = mpsc::channel();
        let prefs = Preferences::load();
        let audio = Audio::new(demo, prefs.volume)?;
        let (media, warning) = if no_media || demo {
            (None, None)
        } else {
            match Media::new() {
                Ok(m) => (Some(m), None),
                Err(e) => (None, Some(format!("Медиакнопки недоступны: {e}"))),
            }
        };
        let now = Instant::now();
        let mut app = Self {
            backend,
            tx,
            rx,
            demo,
            page: Page::empty(Route::Home),
            queue: PlaybackQueue::default(),
            audio,
            focus: Focus::Content,
            nav: 0,
            queue_selected: 0,
            queue_offset: 0,
            show_queue: prefs.show_queue,
            queue_visible: prefs.show_queue,
            help: false,
            input: None,
            input_global: true,
            input_cursor: 0,
            filter: String::new(),
            category: 0,
            loading: false,
            resolving: false,
            status: String::new(),
            status_error: false,
            quit: false,
            hits: vec![],
            liked: HashSet::new(),
            pending_likes: HashSet::new(),
            like_overrides: HashMap::new(),
            likes_ready: false,
            back: vec![],
            forward: vec![],
            page_generation: 0,
            play_generation: 0,
            pause_pending: false,
            radio_generation: 0,
            radio: None,
            radio_pending: false,
            radio_retry: now,
            batches: HashMap::new(),
            radio_name: None,
            radio_waiting: false,
            media,
            media_track: String::new(),
            last_media: now,
            last_click: None,
            status_since: now,
        };
        app.navigate(Route::Home, false);
        let b = app.backend.clone();
        let tx = app.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Reply::LikedIds(b.liked_ids()));
        });
        if let Some(w) = warning {
            app.message(w, true);
        }
        Ok(app)
    }
    pub fn message(&mut self, s: String, error: bool) {
        self.status = s;
        self.status_error = error;
        self.status_since = Instant::now();
    }
    pub fn navigate(&mut self, route: Route, remember: bool) {
        if remember {
            self.back.push(self.view());
            self.forward.clear();
        }
        match &route {
            Route::Home => self.nav = 0,
            Route::Radio => self.nav = 1,
            Route::Liked => self.nav = 2,
            Route::Playlists | Route::Playlist(_, _) => self.nav = 3,
            Route::Search(_) => self.nav = 4,
            Route::Queue => self.nav = 5,
            _ => (),
        }
        self.page_generation += 1;
        self.filter.clear();
        self.category = 0;
        self.page = Page::empty(route.clone());
        self.focus = Focus::Content;
        if route == Route::Queue {
            self.sync_queue_page();
            self.loading = false;
            return;
        }
        self.loading = true;
        let id = self.page_generation;
        let b = self.backend.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Reply::Page(id, b.page(route)));
        });
    }
    fn history(&mut self, forward: bool) {
        let page = if forward {
            self.forward.pop()
        } else {
            self.back.pop()
        };
        if let Some(p) = page {
            if forward {
                self.back.push(self.view());
            } else {
                self.forward.push(self.view());
            }
            self.page_generation += 1;
            self.page = p.page;
            self.nav = p.nav;
            self.filter = p.filter;
            self.category = p.category;
            self.loading = p.loading;
            self.focus = Focus::Content;
            if self.loading {
                self.navigate(self.page.route.clone(), false);
            }
            self.sync_queue_page();
        }
    }
    fn view(&self) -> View {
        View {
            page: self.page.clone(),
            filter: self.filter.clone(),
            category: self.category,
            nav: self.nav,
            loading: self.loading,
        }
    }
    fn open_input(&mut self, global: bool) {
        self.input_global = global;
        let value = if global {
            if let Route::Search(q) = &self.page.route {
                q.clone()
            } else {
                String::new()
            }
        } else {
            self.filter.clone()
        };
        self.input_cursor = value.len();
        self.input = Some(value);
    }
    pub fn paste(&mut self, value: &str) {
        if let Some(input) = self.input.as_mut() {
            let value: String = value
                .chars()
                .map(|c| if c.is_whitespace() { ' ' } else { c })
                .filter(|c| !c.is_control())
                .collect();
            self.input_cursor = self.input_cursor.min(input.len());
            input.insert_str(self.input_cursor, &value);
            self.input_cursor += value.len();
            if !self.input_global {
                self.filter = input.clone();
                self.page.selected = 0;
            }
        }
    }
    pub fn visible(&self) -> Vec<usize> {
        self.page
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| match i {
                Item::Station(s) => s.matches(CATEGORIES[self.category], &self.filter),
                _ => normalized(&format!("{} {}", i.title(), i.subtitle()))
                    .contains(&normalized(&self.filter)),
            })
            .map(|(i, _)| i)
            .collect()
    }
    pub fn selected(&self) -> Option<Item> {
        if self.focus == Focus::Queue {
            return self
                .queue
                .tracks
                .get(self.queue_selected)
                .cloned()
                .map(Item::Track);
        }
        self.visible()
            .get(self.page.selected)
            .and_then(|i| self.page.items.get(*i))
            .cloned()
    }
    fn selected_track(&self) -> Option<Track> {
        match self.selected() {
            Some(Item::Track(t)) => Some(t),
            _ => self.queue.current().cloned(),
        }
    }
    fn sync_queue_page(&mut self) {
        if self.page.route == Route::Queue {
            self.page.title = "Очередь воспроизведения".into();
            self.page.description =
                "Enter Слушать · Delete Удалить · J/K Переставить · e Добавить трек".into();
            self.page.items = self.queue.tracks.iter().cloned().map(Item::Track).collect();
            self.page.selected = self
                .page
                .selected
                .min(self.page.items.len().saturating_sub(1));
        }
    }
    fn reset_radio(&mut self) {
        self.radio_generation += 1;
        self.radio = None;
        self.radio_name = None;
        self.radio_pending = false;
        self.radio_waiting = false;
        self.batches.clear();
    }
    pub fn activate(&mut self) {
        if self.focus == Focus::Nav {
            self.open_nav(self.nav);
            return;
        }
        if self.focus == Focus::Queue || self.page.route == Route::Queue {
            let i = if self.focus == Focus::Queue {
                self.queue_selected
            } else {
                self.page.selected
            };
            if i < self.queue.tracks.len() {
                self.feedback("skip");
                self.queue.index = Some(i);
                self.start_current();
            }
            return;
        }
        match self.selected() {
            Some(Item::Track(t)) => {
                self.feedback("skip");
                self.reset_radio();
                let tracks: Vec<_> = self
                    .visible()
                    .iter()
                    .filter_map(|i| match &self.page.items[*i] {
                        Item::Track(t) => Some(t.clone()),
                        _ => None,
                    })
                    .collect();
                let index = tracks.iter().position(|x| x.id == t.id).unwrap_or(0);
                self.queue.replace(tracks, index);
                self.start_current();
            }
            Some(Item::Artist(a)) => self.navigate(Route::Artist(a.id), true),
            Some(Item::Album(a)) => self.navigate(Route::Album(a.id), true),
            Some(Item::Playlist(p)) => self.navigate(Route::Playlist(p.uid, p.kind), true),
            Some(Item::Station(s)) => self.open_radio(s),
            None => (),
        }
    }
    pub fn open_nav(&mut self, i: usize) {
        match i {
            0 => self.navigate(Route::Home, true),
            1 => self.navigate(Route::Radio, true),
            2 => self.navigate(Route::Liked, true),
            3 => self.navigate(Route::Playlists, true),
            4 => self.open_input(true),
            5 => self.navigate(Route::Queue, true),
            _ => (),
        }
    }
    fn open_artist(&mut self) {
        match self.selected() {
            Some(Item::Artist(a)) => self.navigate(Route::Artist(a.id), true),
            _ => {
                if let Some(t) = self.selected_track() {
                    self.track_artist(t);
                }
            }
        }
    }
    fn track_artist(&mut self, t: Track) {
        if t.artists.len() == 1 {
            self.navigate(Route::Artist(t.artists[0].id.clone()), true);
        } else if !t.artists.is_empty() {
            self.back.push(self.view());
            self.forward.clear();
            self.page_generation += 1;
            self.loading = false;
            self.filter.clear();
            self.category = 0;
            self.page = Page {
                route: Route::Search(t.title.clone()),
                title: format!("Исполнители · {}", t.title),
                description: "Выберите исполнителя и нажмите Enter".into(),
                items: t.artists.into_iter().map(Item::Artist).collect(),
                selected: 0,
                offset: 0,
            };
        }
    }
    fn open_album(&mut self) {
        if let Some(t) = self.selected_track()
            && !t.album.id.is_empty()
        {
            self.navigate(Route::Album(t.album.id), true);
        }
    }
    fn start_current(&mut self) {
        let Some(track) = self.queue.current().cloned() else {
            return;
        };
        self.play_generation += 1;
        self.resolving = true;
        self.pause_pending = false;
        self.radio_waiting = false;
        if let Err(e) = self.audio.stop() {
            self.message(e.to_string(), true);
        }
        let generation = self.play_generation;
        let tx = self.tx.clone();
        let b = self.backend.clone();
        std::thread::spawn(move || {
            let result = b.stream(&track.id);
            let _ = tx.send(Reply::Stream(generation, track, result));
        });
    }
    fn next(&mut self, natural: bool) {
        self.feedback(if natural { "trackFinished" } else { "skip" });
        if self.queue.advance(natural) {
            self.start_current();
        } else if self.radio.is_some() {
            let _ = self.audio.stop();
            self.radio_waiting = true;
            self.fetch_radio(false);
        } else {
            self.play_generation += 1;
            self.resolving = false;
            let _ = self.audio.stop();
            self.message(
                "Очередь закончилась. Выберите музыку или запустите волну".into(),
                false,
            );
        }
    }
    fn previous(&mut self) {
        if self.audio.position() > 3.0 {
            self.action(|s| s.audio.seek(0.0));
        } else {
            self.feedback("skip");
            self.queue.previous();
            self.start_current();
        }
    }
    fn pause(&mut self) {
        if self.resolving {
            self.pause_pending = !self.pause_pending;
            self.message(
                if self.pause_pending {
                    "Трек загрузится на паузе"
                } else {
                    "Трек начнёт играть после загрузки"
                }
                .into(),
                false,
            );
        } else if self.audio.playing {
            let v = !self.audio.paused;
            self.action(|s| s.audio.pause(v));
        } else if self.queue.current().is_some() {
            self.start_current();
        } else {
            self.activate();
        }
    }
    fn stop(&mut self) {
        self.feedback("skip");
        self.play_generation += 1;
        self.resolving = false;
        self.pause_pending = false;
        self.reset_radio();
        self.action(|s| s.audio.stop());
        self.message(
            "Воспроизведение остановлено · пробел — продолжить".into(),
            false,
        );
    }
    fn action(&mut self, f: impl FnOnce(&mut Self) -> Result<()>) {
        if let Err(e) = f(self) {
            self.message(e.to_string(), true);
        }
    }
    fn open_radio(&mut self, station: Station) {
        self.feedback("skip");
        self.reset_radio();
        self.play_generation += 1;
        self.resolving = false;
        let _ = self.audio.stop();
        self.queue.replace(vec![], 0);
        self.radio_name = Some(station.title.clone());
        self.radio = Some((station, String::new()));
        self.radio_waiting = true;
        self.fetch_radio(true);
    }
    fn fetch_radio(&mut self, initial: bool) {
        if self.radio_pending {
            return;
        }
        let Some((station, session)) = self.radio.clone() else {
            return;
        };
        self.radio_pending = true;
        let tx = self.tx.clone();
        let b = self.backend.clone();
        let generation = self.radio_generation;
        let history = self
            .queue
            .tracks
            .iter()
            .rev()
            .take(100)
            .map(|t| t.id.clone())
            .collect::<Vec<_>>();
        std::thread::spawn(move || {
            let r = b.wave(&station.id, &session, &history);
            let _ = tx.send(Reply::Wave(generation, station, initial, r));
        });
    }
    fn feedback(&mut self, event: &str) {
        let Some((_, session)) = &self.radio else {
            return;
        };
        let track = self
            .queue
            .current()
            .map(|t| t.id.clone())
            .unwrap_or_default();
        let batch = self.batches.get(&track).cloned().unwrap_or_default();
        let b = self.backend.clone();
        let session = session.clone();
        let event = event.to_owned();
        let position = self.audio.position();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Reply::Feedback(
                b.feedback(&session, &batch, &event, &track, position),
            ));
        });
    }
    fn like(&mut self) {
        if !self.likes_ready {
            self.message(
                "Отметки ещё загружаются. Попробуйте через несколько секунд; F5 — повторить".into(),
                false,
            );
            return;
        }
        let Some(t) = self.selected_track() else {
            return;
        };
        if !self.pending_likes.insert(t.id.clone()) {
            return;
        }
        let liked = !self.liked.contains(&t.id);
        let b = self.backend.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let r = b.set_like(&t.id, liked);
            let _ = tx.send(Reply::Like(t.id, liked, r));
        });
    }
    pub fn pump(&mut self) {
        while let Ok(reply) = self.rx.try_recv() {
            match reply {
                Reply::Page(id, result) if id == self.page_generation => {
                    self.loading = false;
                    match result {
                        Ok(p) => {
                            if p.route == Route::Liked {
                                self.liked = p
                                    .items
                                    .iter()
                                    .filter_map(|i| {
                                        if let Item::Track(t) = i {
                                            Some(t.id.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                self.likes_ready = true;
                                self.apply_like_overrides();
                            }
                            self.page = p;
                        }
                        Err(e) => {
                            self.page.title = "Не удалось загрузить страницу".into();
                            self.page.description = "F5 — повторить · Alt+← — вернуться".into();
                            self.message(e.to_string(), true);
                        }
                    }
                }
                Reply::Stream(id, t, result) if id == self.play_generation => {
                    self.resolving = false;
                    match result {
                        Ok(url) => match self.audio.play(&url, t.duration()) {
                            Ok(()) => {
                                if self.pause_pending {
                                    self.action(|s| s.audio.pause(true));
                                }
                                self.message(
                                    format!("Сейчас играет: {} — {}", t.artist(), t.title),
                                    false,
                                );
                                self.feedback("trackStarted");
                            }
                            Err(e) => self.message(e.to_string(), true),
                        },
                        Err(e) => self.message(e.to_string(), true),
                    }
                }
                Reply::Wave(id, station, initial, result) if id == self.radio_generation => {
                    self.radio_pending = false;
                    self.radio_retry = Instant::now() + Duration::from_secs(15);
                    match result {
                        Ok(w) => {
                            self.radio = Some((station, w.session));
                            let before = self.queue.tracks.len();
                            let mut seen: HashSet<_> =
                                self.queue.tracks.iter().map(|t| t.id.clone()).collect();
                            for t in w.tracks {
                                if self.demo || seen.insert(t.id.clone()) {
                                    self.batches.insert(t.id.clone(), w.batch.clone());
                                    self.queue.tracks.push(t);
                                }
                            }
                            if initial {
                                self.queue.index = (!self.queue.tracks.is_empty()).then_some(0);
                                self.feedback("radioStarted");
                            }
                            if self.radio_waiting && self.queue.tracks.len() > before {
                                if !initial {
                                    self.queue.index = Some(before);
                                }
                                self.start_current();
                            }
                            if self.queue.tracks.len() == before {
                                self.message(
                                    "Радио пока не вернуло новых треков. Повторим автоматически"
                                        .into(),
                                    false,
                                );
                            }
                        }
                        Err(e) => self.message(format!("Радио: {e}. Повтор через 15 с"), true),
                    }
                }
                Reply::Like(id, liked, result) => {
                    self.pending_likes.remove(&id);
                    match result {
                        Ok(()) => {
                            self.like_overrides.insert(id.clone(), liked);
                            if !liked && self.page.route == Route::Liked {
                                self.page
                                    .items
                                    .retain(|i| !matches!(i,Item::Track(t)if t.id==id));
                            }
                            if liked {
                                self.liked.insert(id);
                            } else {
                                self.liked.remove(&id);
                            }
                            self.message(
                                if liked {
                                    "Добавлено в любимое"
                                } else {
                                    "Удалено из любимого"
                                }
                                .into(),
                                false,
                            );
                        }
                        Err(e) => self.message(e.to_string(), true),
                    }
                }
                Reply::LikedIds(result) => match result {
                    Ok(ids) => {
                        self.liked = ids;
                        self.likes_ready = true;
                        self.apply_like_overrides();
                    }
                    Err(e) => self.message(format!("Не удалось загрузить отметки: {e}"), true),
                },
                Reply::Feedback(Err(e)) => self.message(format!("Обратная связь радио: {e}"), true),
                _ => (),
            }
        }
        match self.audio.tick() {
            Ok(true) => self.next(true),
            Err(e) => self.message(e.to_string(), true),
            _ => (),
        }
        if self.radio.is_some() && !self.radio_pending && Instant::now() >= self.radio_retry {
            let left = self
                .queue
                .tracks
                .len()
                .saturating_sub(self.queue.index.unwrap_or(0) + 1);
            if left <= 3 || self.radio_waiting {
                self.fetch_radio(false);
            }
        }
        self.sync_queue_page();
        self.queue_selected = self
            .queue_selected
            .min(self.queue.tracks.len().saturating_sub(1));
        if self.status_since.elapsed() > Duration::from_secs(12) && !self.status_error {
            self.status.clear();
        }
        self.pump_media();
    }
    fn apply_like_overrides(&mut self) {
        for (id, liked) in &self.like_overrides {
            if *liked {
                self.liked.insert(id.clone());
            } else {
                self.liked.remove(id);
            }
        }
    }
    fn pump_media(&mut self) {
        let commands = self
            .media
            .as_ref()
            .map(|m| m.commands())
            .unwrap_or_default();
        for command in commands {
            use souvlaki::{MediaControlEvent as E, SeekDirection};
            match command {
                E::Play => {
                    if self.resolving {
                        self.pause_pending = false;
                    } else if !self.audio.playing {
                        self.start_current();
                    } else {
                        self.action(|s| s.audio.pause(false));
                    }
                }
                E::Pause => {
                    if self.resolving {
                        self.pause_pending = true;
                    } else {
                        self.action(|s| s.audio.pause(true));
                    }
                }
                E::Toggle => self.pause(),
                E::Next => self.next(false),
                E::Previous => self.previous(),
                E::Stop => self.stop(),
                E::SetPosition(p) => self.action(|s| s.audio.seek(p.0.as_secs_f64())),
                E::SeekBy(d, t) => {
                    let sign = if matches!(d, SeekDirection::Forward) {
                        1.0
                    } else {
                        -1.0
                    };
                    self.action(|s| s.audio.seek(s.audio.position() + sign * t.as_secs_f64()));
                }
                E::Seek(d) => {
                    let sign = if matches!(d, SeekDirection::Forward) {
                        1.0
                    } else {
                        -1.0
                    };
                    self.action(|s| s.audio.seek(s.audio.position() + sign * 10.0));
                }
                E::SetVolume(v) => self.action(|s| s.audio.set_volume(v * 100.0)),
                E::Quit => self.quit = true,
                _ => (),
            }
        }
        if self.last_media.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_media = Instant::now();
        if let Some(media) = self.media.as_mut() {
            let track = self.queue.current();
            let id = track.map(|t| t.id.as_str()).unwrap_or("");
            let result = media.publish(track, &self.audio, self.media_track != id);
            self.media_track = id.into();
            if let Err(e) = result {
                self.message(format!("Медиасессия: {e}"), true);
            }
        }
    }
    fn move_selection(&mut self, delta: isize) {
        match self.focus {
            Focus::Nav => self.nav = self.nav.saturating_add_signed(delta).min(5),
            Focus::Content => {
                self.page.selected = self
                    .page
                    .selected
                    .saturating_add_signed(delta)
                    .min(self.visible().len().saturating_sub(1))
            }
            Focus::Queue => {
                self.queue_selected = self
                    .queue_selected
                    .saturating_add_signed(delta)
                    .min(self.queue.tracks.len().saturating_sub(1))
            }
        }
    }
    fn queue_edit(&mut self, delta: Option<isize>) {
        if self.focus != Focus::Queue && self.page.route != Route::Queue {
            return;
        }
        let i = if self.focus == Focus::Queue {
            self.queue_selected
        } else {
            self.page.selected
        };
        if let Some(d) = delta {
            let n = self.queue.move_item(i, d);
            if self.focus == Focus::Queue {
                self.queue_selected = n;
            } else {
                self.page.selected = n;
            }
        } else {
            let current = self.queue.remove(i);
            if current {
                if self.queue.current().is_some() {
                    self.start_current();
                } else {
                    self.play_generation += 1;
                    self.resolving = false;
                    self.action(|s| s.audio.stop());
                }
            }
        }
        self.sync_queue_page();
    }
    pub fn key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let code = shortcut_code(key.code);
        if key.kind == KeyEventKind::Repeat
            && self.input.is_none()
            && !matches!(
                code,
                KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Char('j' | 'k' | '+' | '=' | '-')
            )
        {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if let KeyCode::Media(media) = code {
            match media {
                MediaKeyCode::PlayPause => self.pause(),
                MediaKeyCode::Play => {
                    if self.audio.paused {
                        self.action(|s| s.audio.pause(false));
                    } else if !self.audio.playing && !self.resolving {
                        self.start_current();
                    }
                }
                MediaKeyCode::Pause => {
                    if self.resolving {
                        self.pause_pending = true;
                    } else {
                        self.action(|s| s.audio.pause(true));
                    }
                }
                MediaKeyCode::Stop => self.stop(),
                MediaKeyCode::TrackNext => self.next(false),
                MediaKeyCode::TrackPrevious => self.previous(),
                _ => (),
            }
            return;
        }
        if self.help {
            if matches!(
                code,
                KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('?') | KeyCode::Char('q')
            ) {
                self.help = false;
            }
            return;
        }
        if self.input.is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.input = None;
                }
                KeyCode::Enter => {
                    let q = self.input.take().unwrap();
                    if !self.input_global {
                        self.filter = q;
                        self.page.selected = 0;
                    } else if !q.trim().is_empty() {
                        self.navigate(Route::Search(q), true);
                    }
                }
                KeyCode::Backspace => {
                    let input = self.input.as_mut().unwrap();
                    self.input_cursor = self.input_cursor.min(input.len());
                    if let Some((previous, _)) =
                        input[..self.input_cursor].char_indices().next_back()
                    {
                        input.drain(previous..self.input_cursor);
                        self.input_cursor = previous;
                    }
                }
                KeyCode::Delete => {
                    let input = self.input.as_mut().unwrap();
                    if self.input_cursor < input.len() {
                        input.remove(self.input_cursor);
                    }
                }
                KeyCode::Left => {
                    let input = self.input.as_ref().unwrap();
                    self.input_cursor = input[..self.input_cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                }
                KeyCode::Right => {
                    let input = self.input.as_ref().unwrap();
                    if let Some(c) = input[self.input_cursor..].chars().next() {
                        self.input_cursor += c.len_utf8();
                    }
                }
                KeyCode::Home => self.input_cursor = 0,
                KeyCode::End => self.input_cursor = self.input.as_ref().unwrap().len(),
                KeyCode::Char(_) if ctrl && code == KeyCode::Char('u') => {
                    self.input.as_mut().unwrap().clear();
                    self.input_cursor = 0;
                }
                KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                    self.input.as_mut().unwrap().insert(self.input_cursor, c);
                    self.input_cursor += c.len_utf8();
                }
                _ => (),
            }
            if !self.input_global
                && let Some(q) = &self.input
            {
                self.filter = q.clone();
                self.page.selected = 0;
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            match code {
                KeyCode::Left => self.history(false),
                KeyCode::Right => self.history(true),
                _ => (),
            }
            return;
        }
        match code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') | KeyCode::F(1) => self.help = true,
            KeyCode::Tab | KeyCode::BackTab => {
                let order = if self.queue_visible {
                    vec![Focus::Nav, Focus::Content, Focus::Queue]
                } else {
                    vec![Focus::Nav, Focus::Content]
                };
                let i = order.iter().position(|f| *f == self.focus).unwrap_or(0);
                self.focus = order[(i + if code == KeyCode::BackTab {
                    order.len() - 1
                } else {
                    1
                }) % order.len()];
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Char('g') | KeyCode::Home => self.move_selection(isize::MIN),
            KeyCode::Char('G') | KeyCode::End => self.move_selection(isize::MAX),
            KeyCode::Enter => self.activate(),
            KeyCode::Char(' ') => self.pause(),
            KeyCode::Char('/') => {
                self.open_input(self.page.route != Route::Radio);
            }
            KeyCode::Char('f') if ctrl => self.open_input(true),
            KeyCode::Char('a') => self.open_artist(),
            KeyCode::Char('A') => self.open_album(),
            KeyCode::Esc | KeyCode::Backspace => self.history(false),
            KeyCode::Char('n') => self.next(false),
            KeyCode::Char('p') => self.previous(),
            KeyCode::Char('x') => self.stop(),
            KeyCode::Left => self.action(|s| s.audio.seek(s.audio.position() - 5.0)),
            KeyCode::Right => self.action(|s| s.audio.seek(s.audio.position() + 5.0)),
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.action(|s| s.audio.set_volume(s.audio.volume + 5.0))
            }
            KeyCode::Char('-') => self.action(|s| s.audio.set_volume(s.audio.volume - 5.0)),
            KeyCode::Char('r') => self.queue.repeat.cycle(),
            KeyCode::Char('s') => self.queue.shuffle = !self.queue.shuffle,
            KeyCode::Char('l') => self.like(),
            KeyCode::Char('e') => {
                if let Some(t) = self.selected_track() {
                    self.queue.tracks.push(t);
                    self.message("Добавлено в очередь".into(), false);
                }
            }
            KeyCode::Delete => self.queue_edit(None),
            KeyCode::Char('J') => self.queue_edit(Some(1)),
            KeyCode::Char('K') => self.queue_edit(Some(-1)),
            KeyCode::Char('Q') => {
                self.show_queue = !self.show_queue;
                if !self.show_queue && self.focus == Focus::Queue {
                    self.focus = Focus::Content;
                }
            }
            KeyCode::Char('w') => {
                let id = if let Route::Artist(id) = &self.page.route {
                    Some(id.clone())
                } else {
                    self.selected_track()
                        .and_then(|t| t.artists.first().map(|a| a.id.clone()))
                };
                if let Some(id) = id {
                    self.open_radio(Station {
                        id: format!("artist:{id}"),
                        title: "Радио исполнителя".into(),
                        category: "Другое".into(),
                    });
                }
            }
            KeyCode::Char('[') => {
                self.category = (self.category + 4) % 5;
                self.page.selected = 0;
            }
            KeyCode::Char(']') => {
                self.category = (self.category + 1) % 5;
                self.page.selected = 0;
            }
            KeyCode::F(5) => {
                if !self.likes_ready {
                    let b = self.backend.clone();
                    let tx = self.tx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(Reply::LikedIds(b.liked_ids()));
                    });
                }
                self.navigate(self.page.route.clone(), false);
            }
            KeyCode::Char(c @ '1'..='6') => self.open_nav(c as usize - '1' as usize),
            _ => (),
        }
    }
    pub fn mouse(&mut self, event: MouseEvent) {
        if self.help {
            if matches!(event.kind, MouseEventKind::Down(_)) {
                self.help = false;
            }
            return;
        }
        if self.input.is_some() {
            return;
        }
        let target = self
            .hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains((event.column, event.row).into()))
            .cloned();
        if let Some((rect, hit)) = target {
            if matches!(
                event.kind,
                MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
            ) {
                let delta = if event.kind == MouseEventKind::ScrollDown {
                    -5.0
                } else {
                    5.0
                };
                if matches!(hit, Hit::Volume) {
                    self.action(|s| s.audio.set_volume(s.audio.volume + delta));
                    return;
                }
                if matches!(hit, Hit::Seek) {
                    self.action(|s| s.audio.seek(s.audio.position() - delta));
                    return;
                }
                self.focus = match hit {
                    Hit::Panel(f) => f,
                    Hit::Nav(_) => Focus::Nav,
                    Hit::Queue(_) => Focus::Queue,
                    _ => Focus::Content,
                };
                self.move_selection(if event.kind == MouseEventKind::ScrollDown {
                    3
                } else {
                    -3
                });
                return;
            }
            if event.kind == MouseEventKind::Down(MouseButton::Right) {
                self.history(false);
                return;
            }
            if !matches!(
                event.kind,
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            ) {
                return;
            }
            if matches!(event.kind, MouseEventKind::Drag(_))
                && !matches!(hit, Hit::Seek | Hit::Volume)
            {
                return;
            }
            let double = self
                .last_click
                .map(|(x, y, t)| {
                    x == event.column && y == event.row && t.elapsed() < Duration::from_millis(400)
                })
                .unwrap_or(false);
            self.last_click = if double {
                None
            } else {
                Some((event.column, event.row, Instant::now()))
            };
            match hit {
                Hit::Panel(f) => self.focus = f,
                Hit::Nav(i) => self.open_nav(i),
                Hit::Row(i) => {
                    self.focus = Focus::Content;
                    self.page.selected = i;
                    if double {
                        self.activate();
                    }
                }
                Hit::RowArtist(i) => {
                    self.focus = Focus::Content;
                    self.page.selected = i;
                    self.open_artist();
                }
                Hit::RowAlbum(i) => {
                    self.focus = Focus::Content;
                    self.page.selected = i;
                    self.open_album();
                }
                Hit::Queue(i) => {
                    self.focus = Focus::Queue;
                    self.queue_selected = i;
                    if double {
                        self.activate();
                    }
                }
                Hit::Seek => {
                    let fraction = (event.column.saturating_sub(rect.x)) as f64
                        / rect.width.saturating_sub(1).max(1) as f64;
                    let d = self.queue.current().map(Track::duration).unwrap_or(0.0);
                    self.action(|s| s.audio.seek(d * fraction));
                }
                Hit::Previous => self.previous(),
                Hit::Pause => self.pause(),
                Hit::Next => self.next(false),
                Hit::Volume => {
                    let v = event.column.saturating_sub(rect.x) as f64
                        / rect.width.saturating_sub(1).max(1) as f64
                        * 100.0;
                    self.action(|s| s.audio.set_volume(v));
                }
                Hit::Repeat => self.queue.repeat.cycle(),
                Hit::Shuffle => self.queue.shuffle = !self.queue.shuffle,
                Hit::Help => self.help = true,
                Hit::Search => self.open_input(true),
                Hit::QueueToggle => self.show_queue = !self.show_queue,
                Hit::Back => self.history(false),
                Hit::Forward => self.history(true),
                Hit::Category(i) => {
                    self.category = i;
                    self.page.selected = 0;
                }
            }
        }
    }
    pub fn save(&self) -> Result<()> {
        Preferences {
            volume: self.audio.volume,
            show_queue: self.show_queue,
        }
        .save()
    }
}

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}
pub fn run(app: &mut App) -> Result<()> {
    enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    while !app.quit {
        app.pump();
        terminal.draw(|f| crate::ui::draw(f, app))?;
        if event::poll(Duration::from_millis(60))? {
            match event::read()? {
                Event::Key(k) => app.key(k),
                Event::Mouse(m) => app.mouse(m),
                Event::Paste(value) => app.paste(&value),
                _ => (),
            }
        }
    }
    app.save()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settle(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            app.pump();
            if !app.loading && !app.resolving && !app.radio_pending && app.pending_likes.is_empty()
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("background operation timed out");
    }
    fn app() -> App {
        let mut a = App::new(None, true, true).unwrap();
        settle(&mut a);
        a
    }
    fn key(a: &mut App, c: KeyCode) {
        a.key(KeyEvent::new(c, KeyModifiers::NONE));
    }
    #[test]
    fn russian_shortcuts_match_english_command_keys() {
        for (ru, en) in "йЙолпПфФтзчкыдуОЛцхъ.,саг"
            .chars()
            .zip("qQjkgGaAnpxrsleJKw[]/?cfu".chars())
        {
            assert_eq!(shortcut_code(KeyCode::Char(ru)), KeyCode::Char(en));
        }
        for code in [KeyCode::Left, KeyCode::F(1), KeyCode::Char('?')] {
            assert_eq!(shortcut_code(code), code);
        }
    }
    #[test]
    fn russian_categories_cycle_in_both_directions() {
        let mut a = app();
        a.navigate(Route::Radio, true);
        settle(&mut a);
        for expected in [1, 2, 3, 4, 0] {
            a.page.selected = 2;
            key(&mut a, KeyCode::Char('ъ'));
            assert_eq!(a.category, expected);
            assert_eq!(a.page.selected, 0);
        }
        key(&mut a, KeyCode::Char('х'));
        assert_eq!(a.category, 4);
    }
    #[test]
    fn russian_commands_preserve_text_and_support_control_and_shift() {
        let mut a = app();
        let visible = a.show_queue;
        a.key(KeyEvent::new(KeyCode::Char('Й'), KeyModifiers::SHIFT));
        assert_eq!(a.show_queue, !visible);
        key(&mut a, KeyCode::Char(','));
        assert!(a.help);
        key(&mut a, KeyCode::Char('й'));
        assert!(!a.help);
        assert!(!a.quit);
        a.key(KeyEvent::new(KeyCode::Char('а'), KeyModifiers::CONTROL));
        assert!(a.input_global);
        let text = "йЙхъ.,ёжмузыка";
        for c in text.chars() {
            key(&mut a, KeyCode::Char(c));
        }
        assert_eq!(a.input.as_deref(), Some(text));
        assert!(!a.quit);
        a.key(KeyEvent::new(KeyCode::Char('г'), KeyModifiers::CONTROL));
        assert_eq!(a.input.as_deref(), Some(""));
        a.key(KeyEvent::new(KeyCode::Char('с'), KeyModifiers::CONTROL));
        assert!(a.quit);
    }
    #[test]
    fn russian_filter_and_repeated_navigation_work() {
        let mut a = app();
        a.navigate(Route::Radio, true);
        settle(&mut a);
        key(&mut a, KeyCode::Char('.'));
        assert!(!a.input_global);
        for c in "Рок".chars() {
            key(&mut a, KeyCode::Char(c));
        }
        assert_eq!(a.filter, "Рок");
        key(&mut a, KeyCode::Esc);
        a.filter.clear();
        a.focus = Focus::Content;
        a.page.selected = 0;
        let mut repeated = KeyEvent::new(KeyCode::Char('о'), KeyModifiers::NONE);
        repeated.kind = KeyEventKind::Repeat;
        a.key(repeated);
        assert_eq!(a.page.selected, 1);
        key(&mut a, KeyCode::Char('л'));
        assert_eq!(a.page.selected, 0);
    }
    #[test]
    fn artist_and_album_open_from_track_and_history_restores_selection() {
        let mut a = app();
        a.page.selected = 1;
        key(&mut a, KeyCode::Char('a'));
        settle(&mut a);
        assert_eq!(a.page.route, Route::Artist("aster-city".into()));
        a.history(false);
        assert_eq!(a.page.route, Route::Home);
        assert_eq!(a.page.selected, 1);
        key(&mut a, KeyCode::Char('A'));
        settle(&mut a);
        assert_eq!(a.page.route, Route::Album("night-signals".into()));
    }
    #[test]
    fn stale_page_result_cannot_overwrite_new_route() {
        let mut a = app();
        let old = a.page_generation;
        a.navigate(Route::Liked, true);
        a.tx.send(Reply::Page(old, Ok(Page::empty(Route::Radio))))
            .unwrap();
        settle(&mut a);
        assert_eq!(a.page.route, Route::Liked);
    }
    #[test]
    fn stale_stream_cannot_restart_old_track() {
        let mut a = app();
        a.page.selected = 1;
        a.activate();
        let old = a.play_generation;
        a.page.selected = 2;
        a.activate();
        a.tx.send(Reply::Stream(
            old,
            crate::api::demo_tracks()[0].clone(),
            Ok("demo://1".into()),
        ))
        .unwrap();
        settle(&mut a);
        assert_eq!(a.queue.current().unwrap().id, "2");
        assert!(a.audio.playing);
    }
    #[test]
    fn radio_refills_and_switch_ignores_stale_batches() {
        let mut a = app();
        a.open_radio(Station::my_wave());
        settle(&mut a);
        assert_eq!(a.queue.tracks.len(), 12);
        a.queue.index = Some(11);
        a.next(false);
        settle(&mut a);
        assert_eq!(a.queue.index, Some(12));
        assert!(a.audio.playing);
        let old = a.radio_generation;
        a.reset_radio();
        a.tx.send(Reply::Wave(
            old,
            Station::my_wave(),
            false,
            Ok(Wave {
                tracks: crate::api::demo_tracks(),
                session: "stale".into(),
                batch: "stale".into(),
            }),
        ))
        .unwrap();
        let len = a.queue.tracks.len();
        a.pump();
        assert_eq!(a.queue.tracks.len(), len);
        assert!(a.radio.is_none());
    }
    #[test]
    fn natural_eof_advances_exactly_once() {
        let mut a = app();
        a.page.selected = 1;
        a.activate();
        settle(&mut a);
        a.audio.seek(a.queue.current().unwrap().duration()).unwrap();
        a.pump();
        settle(&mut a);
        assert_eq!(a.queue.index, Some(1));
        assert!(a.audio.playing);
        a.pump();
        assert_eq!(a.queue.index, Some(1));
    }
    #[test]
    fn search_input_does_not_trigger_playback_shortcuts() {
        let mut a = app();
        key(&mut a, KeyCode::Char('/'));
        for c in "Northern Light".chars() {
            key(&mut a, KeyCode::Char(c));
        }
        assert!(!a.audio.playing);
        key(&mut a, KeyCode::Enter);
        settle(&mut a);
        assert!(a.page.items.iter().any(|i| matches!(i, Item::Artist(_))));
        assert_eq!(a.page.route, Route::Search("Northern Light".into()));
    }
    #[test]
    fn station_filter_and_category_intersect() {
        let mut a = app();
        a.navigate(Route::Radio, true);
        settle(&mut a);
        a.category = 2;
        key(&mut a, KeyCode::Char('/'));
        for c in "веселое".chars() {
            key(&mut a, KeyCode::Char(c));
        }
        assert_eq!(a.visible().len(), 1);
        key(&mut a, KeyCode::Enter);
        a.activate();
        settle(&mut a);
        assert_eq!(a.radio.as_ref().unwrap().0.id, "mood:fun");
    }
    #[test]
    fn single_click_selects_double_click_plays() {
        let mut a = app();
        a.hits = vec![(Rect::new(10, 10, 20, 1), Hit::Row(1))];
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 10,
            modifiers: KeyModifiers::NONE,
        };
        a.mouse(click);
        assert_eq!(a.page.selected, 1);
        assert!(!a.audio.playing);
        a.mouse(click);
        settle(&mut a);
        assert!(a.audio.playing);
        assert_eq!(a.queue.current().unwrap().id, "1");
    }
    #[test]
    fn removing_current_cancels_pending_playback() {
        let mut a = app();
        a.page.selected = 1;
        a.activate();
        settle(&mut a);
        a.navigate(Route::Queue, true);
        a.page.selected = 0;
        a.queue_edit(None);
        settle(&mut a);
        assert_eq!(a.queue.current().unwrap().id, "2");
    }
    #[test]
    fn collaborators_get_a_picker() {
        let mut a = app();
        let mut t = crate::api::demo_tracks()[0].clone();
        t.artists.push(Artist {
            id: "guest".into(),
            name: "Guest".into(),
        });
        a.track_artist(t);
        assert_eq!(a.page.items.len(), 2);
        assert!(matches!(a.page.items[1], Item::Artist(_)));
    }
    #[test]
    fn global_search_from_radio_is_not_a_station_filter() {
        let mut a = app();
        a.navigate(Route::Radio, true);
        settle(&mut a);
        a.open_nav(4);
        a.paste("Northern Light");
        key(&mut a, KeyCode::Enter);
        settle(&mut a);
        assert_eq!(a.page.route, Route::Search("Northern Light".into()));
        assert!(a.filter.is_empty());
    }
    #[test]
    fn history_restores_category_filter_selection_and_nav() {
        let mut a = app();
        a.navigate(Route::Radio, true);
        settle(&mut a);
        a.category = 2;
        a.filter = "веселое".into();
        a.page.selected = 0;
        a.navigate(Route::Liked, true);
        settle(&mut a);
        a.history(false);
        assert_eq!(a.nav, 1);
        assert_eq!(a.category, 2);
        assert_eq!(a.filter, "веселое");
        assert_eq!(a.visible().len(), 1);
        a.history(true);
        assert_eq!(a.page.route, Route::Liked);
        assert_eq!(a.nav, 2);
        assert!(a.filter.is_empty());
    }
    #[test]
    fn unicode_input_can_be_edited_and_pasted_at_cursor() {
        let mut a = app();
        a.open_input(true);
        a.paste("Рок🎵");
        key(&mut a, KeyCode::Left);
        key(&mut a, KeyCode::Backspace);
        a.paste("ка");
        assert_eq!(a.input.as_deref(), Some("Рока🎵"));
        key(&mut a, KeyCode::Delete);
        assert_eq!(a.input.as_deref(), Some("Рока"));
        key(&mut a, KeyCode::Home);
        a.paste("Я\n\u{1b}");
        assert_eq!(a.input.as_deref(), Some("Я Рока"));
    }
    #[test]
    fn pause_during_resolution_does_not_start_another_request() {
        let mut a = app();
        a.page.selected = 1;
        a.activate();
        let generation = a.play_generation;
        key(&mut a, KeyCode::Char(' '));
        assert_eq!(a.play_generation, generation);
        settle(&mut a);
        assert!(a.audio.playing);
        assert!(a.audio.paused);
    }
    #[test]
    fn stop_cancels_inflight_radio_and_stream() {
        let mut a = app();
        a.open_radio(Station::my_wave());
        key(&mut a, KeyCode::Char('x'));
        settle(&mut a);
        assert!(!a.audio.playing);
        assert!(a.radio.is_none());
        a.page.selected = 1;
        a.activate();
        key(&mut a, KeyCode::Char('x'));
        settle(&mut a);
        assert!(!a.audio.playing);
        assert!(!a.resolving);
    }
    #[test]
    fn autorepeat_does_not_toggle_pause_again() {
        let mut a = app();
        a.page.selected = 1;
        a.activate();
        settle(&mut a);
        key(&mut a, KeyCode::Char(' '));
        let mut repeated = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        repeated.kind = KeyEventKind::Repeat;
        a.key(repeated);
        assert!(a.audio.paused);
    }
    #[test]
    fn volume_slider_reaches_one_hundred_and_supports_drag() {
        let mut a = app();
        a.hits = vec![(Rect::new(10, 10, 10, 1), Hit::Volume)];
        a.mouse(MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 19,
            row: 10,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(a.audio.volume, 100.0);
    }
    #[test]
    fn stale_like_snapshot_preserves_acknowledged_mutation() {
        let mut a = app();
        a.like_overrides.insert("1".into(), false);
        a.tx.send(Reply::LikedIds(Ok(["1".into(), "3".into()].into())))
            .unwrap();
        a.pump();
        assert!(!a.liked.contains("1"));
        assert!(a.liked.contains("3"));
    }
    #[test]
    fn unlike_removes_row_from_liked_page() {
        let mut a = app();
        a.navigate(Route::Liked, true);
        settle(&mut a);
        let id = match a.selected().unwrap() {
            Item::Track(t) => t.id,
            _ => panic!("track expected"),
        };
        a.like();
        settle(&mut a);
        assert!(
            !a.page
                .items
                .iter()
                .any(|i| matches!(i,Item::Track(t)if t.id==id))
        );
    }
}
