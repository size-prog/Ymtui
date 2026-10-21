mod api;
mod app;
mod audio;
mod config;
mod media;
mod model;
mod ui;

use anyhow::Result;
use clap::Parser;
#[derive(Parser)]
#[command(version, about = "Яндекс Музыка в терминале · Rust / Ratatui")]
struct Args {
    #[arg(long, help = "Офлайн-демо без звука и авторизации")]
    demo: bool,
    #[arg(long, help = "Вход через код устройства Яндекса")]
    login: bool,
    #[arg(long, help = "Удалить сохранённый токен")]
    logout: bool,
    #[arg(long, help = "Проверить libmpv")]
    check_audio: bool,
    #[arg(long, help = "Проверить системные медиакнопки")]
    check_media: bool,
    #[arg(long, help = "Отключить интеграцию с системным плеером")]
    no_media: bool,
    #[arg(
        long,
        help = "Сохранить текстовый снимок интерфейса (для проверки)",
        hide = true
    )]
    snapshot: Option<std::path::PathBuf>,
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Ошибка: {e:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args = Args::parse();
    if args.login {
        return api::login();
    }
    if args.logout {
        config::logout()?;
        println!("Сохранённый токен удалён. YM_TOKEN задаётся отдельно.");
        return Ok(());
    }
    if args.check_audio {
        let _a = audio::Audio::new(false, 70.0)?;
        println!("OK: libmpv инициализирована");
        return Ok(());
    }
    if args.check_media {
        let _m = media::Media::new()?;
        println!("OK: системная медиасессия инициализирована");
        return Ok(());
    }
    let token = config::token();
    if !args.demo && token.is_none() {
        anyhow::bail!("Сначала выполните ymtui --login или запустите ymtui --demo");
    }
    let mut app = app::App::new(token.as_deref(), args.demo, args.no_media)?;
    if let Some(path) = args.snapshot {
        return ui::snapshot(&mut app, &path);
    }
    app::run(&mut app)
}
