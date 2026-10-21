//! Direct HTTP adapter. No Python runtime or SDK is used.
use crate::model::*;
use anyhow::{Context, Result, anyhow, bail};
use reqwest::{
    blocking::{Client, RequestBuilder},
    header,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::Mutex,
    time::{Duration, Instant},
};

pub struct Backend {
    http: Client,
    uid: Mutex<Option<String>>,
    demo: bool,
    liked: Mutex<HashSet<String>>,
}
#[derive(Clone, Debug)]
pub struct Wave {
    pub tracks: Vec<Track>,
    pub session: String,
    pub batch: String,
}
fn string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}
fn list(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or_default()
}
fn artist(v: &Value) -> Artist {
    Artist {
        id: string(&v["id"]),
        name: string(&v["name"]),
    }
}
fn album(v: &Value) -> Album {
    Album {
        id: string(&v["id"]),
        title: string(&v["title"]),
        year: v["year"].as_u64().unwrap_or(0),
    }
}
pub fn track(v: &Value) -> Track {
    let a = &v["albums"][0];
    let cover = v["coverUri"]
        .as_str()
        .or(a["coverUri"].as_str())
        .unwrap_or("");
    Track {
        id: string(&v["id"]),
        title: v["title"].as_str().unwrap_or("Без названия").into(),
        artists: list(&v["artists"]).iter().map(artist).collect(),
        album: album(a),
        duration_ms: v["durationMs"].as_u64().unwrap_or(0),
        cover: if cover.is_empty() {
            String::new()
        } else {
            format!(
                "https://{}",
                cover
                    .trim_start_matches("https://")
                    .trim_start_matches("//")
                    .replace("%%", "400x400")
            )
        },
    }
}
fn track_items(v: &Value) -> Vec<Item> {
    list(v)
        .iter()
        .filter(|v| !string(&v["id"]).is_empty())
        .map(|v| Item::Track(track(v)))
        .collect()
}
impl Backend {
    pub fn new(token: Option<&str>, demo: bool) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        if let Some(t) = token {
            let mut h = header::HeaderValue::from_str(&format!("OAuth {t}"))
                .context("Некорректный токен")?;
            h.set_sensitive(true);
            headers.insert(header::AUTHORIZATION, h);
        }
        let http = Client::builder()
            .default_headers(headers)
            .user_agent("ymtui/1.0")
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .build()?;
        Ok(Self {
            http,
            uid: Mutex::new(None),
            demo,
            liked: Mutex::new(["1".into(), "3".into()].into()),
        })
    }
    fn url(path: &str) -> String {
        format!("https://api.music.yandex.net{path}")
    }
    fn response(&self, request: RequestBuilder) -> Result<Value> {
        let response = request
            .send()
            .map_err(|_| anyhow!("Нет соединения с Яндекс Музыкой. F5 — повторить"))?;
        let status = response.status();
        if !status.is_success() {
            bail!(match status.as_u16() {
                401 => "Сессия истекла. Выполните ymtui --login".into(),
                403 => "Доступ запрещён: проверьте подписку и регион".into(),
                429 => "Слишком много запросов. Повторите позже".into(),
                _ => format!("Ошибка API: HTTP {status}"),
            });
        }
        let v: Value = response.json().context("Не удалось прочитать ответ API")?;
        if !v["error"].is_null() {
            bail!("Яндекс Музыка отклонила запрос");
        }
        Ok(v.get("result").cloned().unwrap_or(v))
    }
    fn get(&self, path: &str) -> Result<Value> {
        self.response(self.http.get(Self::url(path)))
    }
    fn post(&self, path: &str, value: Value) -> Result<Value> {
        self.response(self.http.post(Self::url(path)).json(&value))
    }
    fn uid(&self) -> Result<String> {
        let mut guard = self.uid.lock().unwrap();
        if let Some(uid) = &*guard {
            return Ok(uid.clone());
        }
        let status = self.get("/account/status")?;
        let uid = string(&status["account"]["uid"]);
        if uid.is_empty() {
            bail!("Не удалось определить аккаунт");
        }
        *guard = Some(uid.clone());
        Ok(uid)
    }
    fn resolve(&self, ids: Vec<String>) -> Result<Vec<Item>> {
        let mut items = vec![];
        for chunk in ids.chunks(100) {
            let v = self.response(
                self.http
                    .post(Self::url("/tracks"))
                    .form(&[("track-ids", chunk.join(","))]),
            )?;
            items.extend(track_items(&v));
        }
        Ok(items)
    }
    pub fn liked_ids(&self) -> Result<HashSet<String>> {
        if self.demo {
            return Ok(self.liked.lock().unwrap().clone());
        }
        let v = self.get(&format!("/users/{}/likes/tracks", self.uid()?))?;
        Ok(list(&v["library"]["tracks"])
            .iter()
            .map(|v| string(&v["id"]))
            .filter(|id| !id.is_empty())
            .collect())
    }
    pub fn page(&self, route: Route) -> Result<Page> {
        if self.demo {
            return self.demo_page(route);
        }
        let mut page = Page::empty(route.clone());
        match route {
            Route::Home => {
                self.uid()?;
                page.title = "Хорошая музыка рядом".into();
                page.description =
                    "Выберите настроение. Откройте любимых исполнителей. Слушайте своё.".into();
                page.items = vec![Item::Station(Station::my_wave())];
                let likes = self.get(&format!("/users/{}/likes/tracks", self.uid()?))?;
                let ids = list(&likes["library"]["tracks"])
                    .iter()
                    .take(24)
                    .map(|v| string(&v["id"]))
                    .filter(|id| !id.is_empty())
                    .collect();
                page.items.extend(self.resolve(ids)?);
            }
            Route::Radio => {
                page.title = "Моя волна и радио".into();
                page.description = "/ Фильтр   [ / ] (х / ъ) Категории   Enter Слушать".into();
                page.items = self.stations()?.into_iter().map(Item::Station).collect();
            }
            Route::Liked => {
                page.title = "Мне нравится".into();
                let v = self.get(&format!("/users/{}/likes/tracks", self.uid()?))?;
                let ids: Vec<_> = list(&v["library"]["tracks"])
                    .iter()
                    .map(|v| string(&v["id"]))
                    .filter(|s| !s.is_empty())
                    .collect();
                page.description = format!("{} любимых треков", ids.len());
                page.items = self.resolve(ids)?;
            }
            Route::Playlists => {
                page.title = "Ваши плейлисты".into();
                let v = self.get(&format!("/users/{}/playlists/list", self.uid()?))?;
                page.items = list(&v)
                    .iter()
                    .map(|v| {
                        Item::Playlist(Playlist {
                            uid: string(&v["owner"]["uid"]),
                            kind: string(&v["kind"]),
                            title: string(&v["title"]),
                            count: v["trackCount"].as_u64().unwrap_or(0),
                        })
                    })
                    .collect();
            }
            Route::Search(q) => {
                page.title = format!("Поиск: {q}");
                page.description =
                    "Треки, исполнители и альбомы · Enter — открыть · a — исполнитель трека".into();
                let v = self.response(self.http.get(Self::url("/search")).query(&[
                    ("text", q.as_str()),
                    ("type", "all"),
                    ("page", "0"),
                ]))?;
                page.items.extend(
                    list(&v["artists"]["results"])
                        .iter()
                        .map(|v| Item::Artist(artist(v))),
                );
                page.items.extend(
                    list(&v["albums"]["results"])
                        .iter()
                        .map(|v| Item::Album(album(v))),
                );
                page.items.extend(track_items(&v["tracks"]["results"]));
            }
            Route::Artist(id) => {
                let v = self.get(&format!("/artists/{id}/brief-info"))?;
                page.title = string(&v["artist"]["name"]);
                let genres = list(&v["artist"]["genres"])
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" · ");
                let about = v["artist"]["description"]["text"].as_str().unwrap_or("");
                page.description = format!(
                    "ИСПОЛНИТЕЛЬ · {genres}\n{about}\nПопулярные треки / альбомы / похожие исполнители · w — радио артиста"
                );
                page.items.extend(track_items(&v["popularTracks"]));
                if page.items.is_empty() {
                    page.items.extend(track_items(&v["topTracks"]));
                }
                if page.items.is_empty() {
                    page.items.extend(track_items(
                        &self.get(&format!("/artists/{id}/tracks?page-size=100"))?["tracks"],
                    ));
                }
                page.items
                    .extend(list(&v["albums"]).iter().map(|v| Item::Album(album(v))));
                page.items.extend(
                    list(&v["similarArtists"])
                        .iter()
                        .map(|v| Item::Artist(artist(v))),
                );
            }
            Route::Album(id) => {
                let v = self.get(&format!("/albums/{id}/with-tracks"))?;
                page.title = string(&v["title"]);
                page.description = format!("АЛЬБОМ · {} · {} треков", v["year"], v["trackCount"]);
                for volume in list(&v["volumes"]) {
                    for raw in list(volume) {
                        let mut t = track(raw);
                        if t.album.id.is_empty() {
                            t.album = album(&v);
                        }
                        page.items.push(Item::Track(t));
                    }
                }
            }
            Route::Playlist(uid, kind) => {
                let v = self.get(&format!("/users/{uid}/playlists/{kind}"))?;
                page.title = string(&v["title"]);
                page.items = self.resolve(
                    list(&v["tracks"])
                        .iter()
                        .map(|v| string(&v["id"]))
                        .filter(|s| !s.is_empty())
                        .collect(),
                )?;
            }
            Route::Queue => {
                page.title = "Очередь воспроизведения".into();
            }
        }
        Ok(page)
    }
    fn stations(&self) -> Result<Vec<Station>> {
        let v = self.get("/rotor/stations/list?language=ru")?;
        let mut stations = vec![Station::my_wave()];
        let mut seen: HashSet<String> = [stations[0].id.clone()].into();
        let mut entries = list(&v).to_vec();
        if let Ok(w) = self.get("/rotor/wave/settings") {
            for b in list(&w["blocks"]) {
                entries.extend(list(&b["items"]).iter().cloned());
            }
        }
        for entry in entries {
            let v = entry.get("station").unwrap_or(&entry);
            let kind = string(&v["id"]["type"]);
            let tag = string(&v["id"]["tag"]);
            let id = v["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{kind}:{tag}"));
            if id == ":" || id.starts_with("generative:") || !seen.insert(id.clone()) {
                continue;
            }
            let source = format!("{id} {}", string(&v["idForFrom"])).to_lowercase();
            let category = if source.contains("genre") {
                "Жанры"
            } else if source.contains("mood") {
                "Настроения"
            } else if source.contains("activity") || source.contains("occupation") {
                "Занятия"
            } else {
                "Другое"
            };
            stations.push(Station {
                id,
                title: string(&v["name"]),
                category: category.into(),
            });
        }
        Ok(stations)
    }
    pub fn wave(&self, station: &str, session: &str, history: &[String]) -> Result<Wave> {
        if self.demo {
            return Ok(Wave {
                tracks: demo_tracks(),
                session: "demo".into(),
                batch: "demo".into(),
            });
        }
        let v = if session.is_empty() {
            self.post(
                "/rotor/session/new",
                json!({"seeds":[station], "includeTracksInResponse":true}),
            )?
        } else {
            self.post(
                &format!("/rotor/session/{session}/tracks"),
                json!({"queue": history}),
            )?
        };
        if v["unknownSession"].as_bool() == Some(true) {
            bail!("Сессия радио истекла. Откройте станцию заново");
        }
        let tracks = list(&v["sequence"])
            .iter()
            .filter_map(|v| v.get("track"))
            .map(track)
            .collect();
        Ok(Wave {
            tracks,
            session: v["radioSessionId"].as_str().unwrap_or(session).into(),
            batch: string(&v["batchId"]),
        })
    }
    pub fn feedback(
        &self,
        session: &str,
        batch: &str,
        event: &str,
        track_id: &str,
        played: f64,
    ) -> Result<()> {
        if self.demo || session.is_empty() {
            return Ok(());
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64;
        let mut e = json!({"type":event, "timestamp":timestamp});
        if !track_id.is_empty() {
            e["trackId"] = json!(track_id);
        }
        if event == "skip" || event == "trackFinished" {
            e["totalPlayedSeconds"] = json!(played.max(0.0));
        }
        self.post(
            &format!("/rotor/session/{session}/feedback"),
            json!({"event":e, "batchId":batch, "from":"ymtui"}),
        )?;
        Ok(())
    }
    pub fn set_like(&self, id: &str, liked: bool) -> Result<()> {
        if self.demo {
            let mut set = self.liked.lock().unwrap();
            if liked {
                set.insert(id.into());
            } else {
                set.remove(id);
            }
            return Ok(());
        }
        self.response(
            self.http
                .post(Self::url(&format!(
                    "/users/{}/likes/tracks/{}",
                    self.uid()?,
                    if liked { "add-multiple" } else { "remove" }
                )))
                .form(&[("track-ids", id)]),
        )?;
        Ok(())
    }
    pub fn stream(&self, id: &str) -> Result<String> {
        if self.demo {
            return Ok(format!("demo://{id}"));
        }
        let v = self.get(&format!("/tracks/{id}/download-info"))?;
        let best = list(&v)
            .iter()
            .filter(|v| v["codec"] == "mp3" && v["preview"].as_bool() != Some(true))
            .max_by_key(|v| v["bitrateInKbps"].as_u64().unwrap_or(0))
            .context("Нет доступного MP3 потока. Проверьте подписку и регион")?;
        let url = best["downloadInfoUrl"]
            .as_str()
            .context("Нет адреса потока")?;
        // A separate client prevents OAuth credentials from reaching the CDN.
        let xml = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?
            .get(url)
            .send()
            .map_err(|_| anyhow!("Не удалось получить аудиопоток"))?
            .error_for_status()?
            .text()?;
        direct_link(&xml)
    }
    fn demo_page(&self, route: Route) -> Result<Page> {
        let tracks = demo_tracks();
        let mut p = Page::empty(route.clone());
        match route {
            Route::Home => {
                p.title = "Ваша музыка. Ваш ритм.".into();
                p.description = "Добро пожаловать домой\nНовые открытия, знакомые голоса и музыка для этого момента.".into();
                p.items.push(Item::Station(Station::my_wave()));
                p.items.extend(tracks.into_iter().map(Item::Track));
            }
            Route::Radio => {
                p.title = "Моя волна и радио".into();
                p.description = "Музыка под ваше настроение · / фильтр · [ ] категория".into();
                p.items.push(Item::Station(Station::my_wave()));
                for (id, name, category) in [
                    ("genre:rock", "Рок", "Жанры"),
                    ("genre:jazz", "Джаз", "Жанры"),
                    ("genre:electronic", "Электроника", "Жанры"),
                    ("mood:calm", "Спокойное", "Настроения"),
                    ("mood:fun", "Весёлое", "Настроения"),
                    ("activity:focus", "Концентрация", "Занятия"),
                    ("activity:workout", "Тренировка", "Занятия"),
                ] {
                    p.items.push(Item::Station(Station {
                        id: id.into(),
                        title: name.into(),
                        category: category.into(),
                    }));
                }
            }
            Route::Liked => {
                p.title = "Мне нравится".into();
                p.items = tracks
                    .into_iter()
                    .filter(|t| self.liked.lock().unwrap().contains(&t.id))
                    .map(Item::Track)
                    .collect();
            }
            Route::Playlists => {
                p.title = "Ваши плейлисты".into();
                for (kind, title) in [("1", "Night Drive"), ("2", "Chill & Focus")] {
                    p.items.push(Item::Playlist(Playlist {
                        uid: "demo".into(),
                        kind: kind.into(),
                        title: title.into(),
                        count: 6,
                    }));
                }
            }
            Route::Search(q) => {
                p.title = format!("Поиск: {q}");
                let needle = normalized(&q);
                let mut seen = HashSet::new();
                for t in &tracks {
                    for a in &t.artists {
                        if normalized(&a.name).contains(&needle) && seen.insert(a.id.clone()) {
                            p.items.push(Item::Artist(a.clone()));
                        }
                    }
                }
                p.items.extend(
                    tracks
                        .into_iter()
                        .filter(|t| {
                            normalized(&format!("{} {} {}", t.title, t.artist(), t.album.title))
                                .contains(&needle)
                        })
                        .map(Item::Track),
                );
            }
            Route::Artist(id) => {
                let a = tracks
                    .iter()
                    .flat_map(|t| &t.artists)
                    .find(|a| a.id == id)
                    .cloned()
                    .context("Исполнитель не найден")?;
                p.title = a.name;
                p.description = "ИСПОЛНИТЕЛЬ · Electronic · Indie\nНочные маршруты и тёплый аналоговый звук.\nПопулярные треки / дискография / похожие исполнители · w — радио".into();
                let own: Vec<_> = tracks
                    .iter()
                    .filter(|t| t.artists.iter().any(|a| a.id == id))
                    .cloned()
                    .collect();
                p.items.extend(own.iter().cloned().map(Item::Track));
                let mut seen = HashSet::new();
                for t in own {
                    if seen.insert(t.album.id.clone()) {
                        p.items.push(Item::Album(t.album));
                    }
                }
                let mut seen = HashSet::new();
                for t in tracks {
                    for a in t.artists {
                        if a.id != id && seen.insert(a.id.clone()) {
                            p.items.push(Item::Artist(a));
                        }
                    }
                }
            }
            Route::Album(id) => {
                let a = tracks
                    .iter()
                    .find(|t| t.album.id == id)
                    .context("Альбом не найден")?
                    .album
                    .clone();
                p.title = a.title;
                p.description = format!("АЛЬБОМ · {}", a.year);
                p.items = tracks
                    .into_iter()
                    .filter(|t| t.album.id == id)
                    .map(Item::Track)
                    .collect();
            }
            Route::Playlist(_, kind) => {
                p.title = if kind == "1" {
                    "Night Drive"
                } else {
                    "Chill & Focus"
                }
                .into();
                p.items = tracks
                    .into_iter()
                    .skip(if kind == "1" { 0 } else { 6 })
                    .take(6)
                    .map(Item::Track)
                    .collect();
            }
            Route::Queue => {
                p.title = "Очередь воспроизведения".into();
            }
        }
        Ok(p)
    }
}
fn direct_link(xml: &str) -> Result<String> {
    let doc = roxmltree::Document::parse(xml)?;
    let field = |name| {
        doc.descendants()
            .find(|n| n.has_tag_name(name))
            .and_then(|n| n.text())
            .context("Неполный ответ аудиосервера")
    };
    let host = field("host")?;
    let path = field("path")?;
    let ts = field("ts")?;
    let salt = field("s")?;
    if !path.starts_with('/') || host.contains(['/', '@', ':']) {
        bail!("Некорректный адрес аудиосервера");
    }
    let sign = md5::compute(format!("XGRlBW9FXlekgbPrRHuSiA{}{salt}", &path[1..]));
    Ok(format!("https://{host}/get-mp3/{sign:x}/{ts}{path}"))
}
pub fn demo_tracks() -> Vec<Track> {
    [
        ("Midnight Transit", "Aster City", "Night Signals", 223),
        ("Your Frequency", "Northern Light", "Afterhours", 198),
        ("Signal From Home", "Memory Arcade", "Blue Hour", 251),
        ("Glass Skyline", "Parallel Echo", "Neon Air", 185),
        ("Ocean in Binary", "Null Sunset", "Late Broadcast", 244),
        ("Slow Motion", "Sunday Radio", "Blue Hour", 201),
        ("Over the Rooftops", "Northern Light", "Afterhours", 179),
        ("Paper Planets", "Memory Arcade", "Orbit", 238),
        ("Echoes of Dawn", "Aster City", "Night Signals", 256),
        ("Low Orbit", "Parallel Echo", "Neon Air", 211),
        ("Sleepless", "Sunday Radio", "Late Broadcast", 193),
        ("First Train", "Null Sunset", "Orbit", 218),
    ]
    .iter()
    .enumerate()
    .map(|(i, (title, name, record, secs))| Track {
        id: (i + 1).to_string(),
        title: (*title).into(),
        artists: vec![Artist {
            id: name.replace(' ', "-").to_lowercase(),
            name: (*name).into(),
        }],
        album: Album {
            id: record.replace(' ', "-").to_lowercase(),
            title: (*record).into(),
            year: 2026,
        },
        duration_ms: secs * 1000,
        cover: String::new(),
    })
    .collect()
}

pub fn login() -> Result<()> {
    let http = Client::builder().timeout(Duration::from_secs(20)).build()?;
    let client_id = "23cabbbdc6cd418abb4b39c32c41195d";
    let code: Value = http
        .post("https://oauth.yandex.ru/device/code")
        .form(&[
            ("client_id", client_id),
            ("device_name", "YaMusic TUI Rust"),
        ])
        .send()?
        .error_for_status()?
        .json()?;
    let device = code["device_code"]
        .as_str()
        .context("Не удалось получить код устройства")?;
    let url = code["verification_uri"]
        .as_str()
        .or(code["verification_url"].as_str())
        .unwrap_or("https://oauth.yandex.ru/device");
    println!(
        "Откройте {url}\nКод: {}\nОжидание входа… Ctrl+C — отмена",
        code["user_code"].as_str().unwrap_or("")
    );
    let deadline = Instant::now() + Duration::from_secs(code["expires_in"].as_u64().unwrap_or(300));
    let mut interval = code["interval"].as_u64().unwrap_or(5).max(1);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(interval));
        let v: Value = http
            .post("https://oauth.yandex.ru/token")
            .form(&[
                ("grant_type", "device_code"),
                ("code", device),
                ("client_id", client_id),
                ("client_secret", "53bc75238f0c4d08a118e51fe9203300"),
            ])
            .send()?
            .json()?;
        if let Some(token) = v["access_token"].as_str() {
            crate::config::save_token(token)?;
            println!("Вход выполнен. Запустите ymtui.");
            return Ok(());
        }
        match v["error"].as_str() {
            Some("authorization_pending") => (),
            Some("slow_down") => interval += 5,
            _ => bail!("Вход отклонён или код истёк. Повторите ymtui --login"),
        }
    }
    bail!("Время ожидания входа истекло")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_ids_and_collaborators_survive() {
        let t = track(
            &json!({"id":42,"title":"X","artists":[{"id":1,"name":"A"},{"id":"2","name":"B"}],"albums":[{"id":3,"title":"LP"}],"durationMs":120000}),
        );
        assert_eq!(t.id, "42");
        assert_eq!(t.artist(), "A, B");
        assert_eq!(t.album.id, "3");
    }
    #[test]
    fn demo_artist_album_navigation() {
        let b = Backend::new(None, true).unwrap();
        let p = b.page(Route::Artist("aster-city".into())).unwrap();
        assert_eq!(p.title, "Aster City");
        assert!(p.items.iter().any(|i| matches!(i, Item::Album(_))));
        let p = b.page(Route::Album("night-signals".into())).unwrap();
        assert_eq!(p.items.len(), 2);
    }
    #[test]
    fn stream_signature() {
        let url=direct_link("<download-info><host>cdn.yandex.net</host><path>/abc</path><ts>123</ts><s>salt</s></download-info>").unwrap();
        let sig = md5::compute("XGRlBW9FXlekgbPrRHuSiAabcsalt");
        assert_eq!(
            url,
            format!("https://cdn.yandex.net/get-mp3/{sig:x}/123/abc")
        );
        assert!(direct_link("<x/>").is_err());
    }
}
