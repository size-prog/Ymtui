use serde::{Deserialize, Serialize};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub year: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artists: Vec<Artist>,
    pub album: Album,
    pub duration_ms: u64,
    pub cover: String,
}
impl Track {
    pub fn artist(&self) -> String {
        self.artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
    pub fn duration(&self) -> f64 {
        self.duration_ms as f64 / 1000.0
    }
}

#[derive(Clone, Debug)]
pub struct Station {
    pub id: String,
    pub title: String,
    pub category: String,
}
impl Station {
    pub fn my_wave() -> Self {
        Self {
            id: "user:onyourwave".into(),
            title: "Моя волна".into(),
            category: "Все".into(),
        }
    }
    pub fn matches(&self, category: &str, query: &str) -> bool {
        let haystack = normalized(&format!("{} {} {}", self.title, self.category, self.id));
        (category == "Все" || category == self.category)
            && normalized(query)
                .split_whitespace()
                .all(|q| haystack.contains(q))
    }
}
pub fn normalized(s: &str) -> String {
    s.to_lowercase()
        .replace('ё', "е")
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .collect()
}
pub fn time(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

#[derive(Clone, Debug)]
pub struct Playlist {
    pub uid: String,
    pub kind: String,
    pub title: String,
    pub count: u64,
}

#[derive(Clone, Debug)]
pub enum Item {
    Track(Track),
    Artist(Artist),
    Album(Album),
    Playlist(Playlist),
    Station(Station),
}
impl Item {
    pub fn title(&self) -> String {
        match self {
            Self::Track(t) => t.title.clone(),
            Self::Artist(a) => a.name.clone(),
            Self::Album(a) => a.title.clone(),
            Self::Playlist(p) => p.title.clone(),
            Self::Station(s) => s.title.clone(),
        }
    }
    pub fn subtitle(&self) -> String {
        match self {
            Self::Track(t) => t.artist(),
            Self::Artist(_) => "Исполнитель →".into(),
            Self::Album(a) => format!("Альбом · {} →", a.year),
            Self::Playlist(p) => format!("{} треков →", p.count),
            Self::Station(s) => format!("{} · радио", s.category),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Home,
    Radio,
    Liked,
    Playlists,
    Search(String),
    Artist(String),
    Album(String),
    Playlist(String, String),
    Queue,
}
#[derive(Clone, Debug)]
pub struct Page {
    pub route: Route,
    pub title: String,
    pub description: String,
    pub items: Vec<Item>,
    pub selected: usize,
    pub offset: usize,
}
impl Page {
    pub fn empty(route: Route) -> Self {
        Self {
            route,
            title: "Загрузка…".into(),
            description: String::new(),
            items: vec![],
            selected: 0,
            offset: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}
impl Repeat {
    pub fn cycle(&mut self) {
        *self = match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        };
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "выкл",
            Self::All => "все",
            Self::One => "один",
        }
    }
}
#[derive(Default)]
pub struct PlaybackQueue {
    pub tracks: Vec<Track>,
    pub index: Option<usize>,
    pub repeat: Repeat,
    pub shuffle: bool,
}
impl PlaybackQueue {
    pub fn current(&self) -> Option<&Track> {
        self.index.and_then(|i| self.tracks.get(i))
    }
    pub fn replace(&mut self, tracks: Vec<Track>, index: usize) {
        self.index = (!tracks.is_empty()).then(|| index.min(tracks.len() - 1));
        self.tracks = tracks;
    }
    pub fn advance(&mut self, natural: bool) -> bool {
        let Some(i) = self.index else {
            return false;
        };
        if natural && self.repeat == Repeat::One {
            return true;
        }
        if self.shuffle && self.tracks.len() > 1 {
            let mut next = rand::random_range(0..self.tracks.len() - 1);
            if next >= i {
                next += 1;
            }
            self.index = Some(next);
        } else if i + 1 < self.tracks.len() {
            self.index = Some(i + 1);
        } else if self.repeat == Repeat::All {
            self.index = Some(0);
        } else {
            return false;
        }
        true
    }
    pub fn previous(&mut self) {
        if let Some(i) = self.index {
            self.index = Some(if i > 0 {
                i - 1
            } else if self.repeat == Repeat::All {
                self.tracks.len() - 1
            } else {
                0
            });
        }
    }
    pub fn remove(&mut self, i: usize) -> bool {
        if i >= self.tracks.len() {
            return false;
        }
        let current = self.index == Some(i);
        self.tracks.remove(i);
        self.index = self.index.and_then(|c| {
            if self.tracks.is_empty() {
                None
            } else {
                Some(if i < c {
                    c - 1
                } else {
                    c.min(self.tracks.len() - 1)
                })
            }
        });
        current
    }
    pub fn move_item(&mut self, i: usize, delta: isize) -> usize {
        let other = i.saturating_add_signed(delta);
        if i >= self.tracks.len() || other >= self.tracks.len() {
            return i;
        }
        self.tracks.swap(i, other);
        if self.index == Some(i) {
            self.index = Some(other);
        } else if self.index == Some(other) {
            self.index = Some(i);
        }
        other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn queue() -> PlaybackQueue {
        let mut q = PlaybackQueue::default();
        q.replace(
            (0..3)
                .map(|i| Track {
                    id: i.to_string(),
                    ..Default::default()
                })
                .collect(),
            0,
        );
        q
    }
    #[test]
    fn repeat_one_only_on_eof() {
        let mut q = queue();
        q.repeat = Repeat::One;
        assert!(q.advance(true));
        assert_eq!(q.index, Some(0));
        assert!(q.advance(false));
        assert_eq!(q.index, Some(1));
    }
    #[test]
    fn end_does_not_wrap_without_repeat() {
        let mut q = queue();
        q.index = Some(2);
        assert!(!q.advance(true));
        q.repeat = Repeat::All;
        assert!(q.advance(true));
        assert_eq!(q.index, Some(0));
    }
    #[test]
    fn reorder_preserves_current_identity() {
        let mut q = queue();
        q.move_item(0, 1);
        assert_eq!(q.current().unwrap().id, "0");
        assert!(!q.remove(0));
        assert_eq!(q.current().unwrap().id, "0");
        assert!(q.remove(0));
        assert_eq!(q.index, Some(0));
    }
    #[test]
    fn shuffle_never_repeats_current() {
        let mut q = queue();
        q.shuffle = true;
        for _ in 0..100 {
            let old = q.index;
            q.advance(false);
            assert_ne!(old, q.index);
        }
    }
    #[test]
    fn empty_queue_is_safe() {
        let mut q = queue();
        q.replace(vec![], 99);
        assert!(!q.advance(true));
        q.previous();
        assert!(!q.remove(0));
        assert_eq!(q.current(), None);
    }
    #[test]
    fn unicode_station_filter() {
        let s = Station {
            id: "mood:fun".into(),
            title: "Весёлое".into(),
            category: "Настроения".into(),
        };
        assert!(s.matches("Все", "ВЕСЕЛОЕ настроения"));
        assert!(!s.matches("Жанры", ""));
    }
}
