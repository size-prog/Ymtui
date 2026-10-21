use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

pub fn dir() -> PathBuf {
    if cfg!(windows)
        && let Some(p) = std::env::var_os("APPDATA")
    {
        return PathBuf::from(p).join("ymusic-spotatui");
    }
    if let Some(p) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(p).join("ymusic-spotatui");
    }
    directories::BaseDirs::new()
        .map(|d| d.config_dir().join("ymusic-spotatui"))
        .unwrap_or_else(|| PathBuf::from(".ymtui"))
}
pub fn token() -> Option<String> {
    std::env::var("YM_TOKEN")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| fs::read_to_string(dir().join("token")).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
pub fn save_token(token: &str) -> Result<()> {
    if token.trim().is_empty() || token.contains(['\n', '\r']) {
        bail!("Некорректный токен");
    }
    fs::create_dir_all(dir())?;
    let path = dir().join("token");
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    opts.open(path)
        .context("Не удалось сохранить токен")?
        .write_all(token.trim().as_bytes())?;
    Ok(())
}
pub fn logout() -> Result<()> {
    let p = dir().join("token");
    if p.exists() {
        fs::remove_file(p)?;
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub volume: f64,
    pub show_queue: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            volume: 70.0,
            show_queue: true,
        }
    }
}
impl Preferences {
    pub fn load() -> Self {
        fs::read(dir().join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<()> {
        fs::create_dir_all(dir())?;
        fs::write(
            dir().join("settings.json"),
            serde_json::to_vec_pretty(self)?,
        )?;
        Ok(())
    }
}
