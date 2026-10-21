//! libmpv is loaded at runtime: no platform-specific link-time installation.
use anyhow::{Context, Result, bail};
use libloading::Library;
use std::{
    ffi::{CString, c_char, c_int, c_void},
    path::PathBuf,
    time::Instant,
};

type Create = unsafe extern "C" fn() -> *mut c_void;
type Initialize = unsafe extern "C" fn(*mut c_void) -> c_int;
type OptionString = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
type Command = unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int;
type GetProperty = unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int;
type SetProperty = unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int;
type Destroy = unsafe extern "C" fn(*mut c_void);
type WaitEvent = unsafe extern "C" fn(*mut c_void, f64) -> *const MpvEvent;
#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}
pub struct Audio {
    mpv: Option<Mpv>,
    pub paused: bool,
    pub playing: bool,
    pub volume: f64,
    demo_position: f64,
    tick: Instant,
    duration: f64,
}
struct Mpv {
    handle: *mut c_void,
    command: Command,
    get: GetProperty,
    set: SetProperty,
    wait: WaitEvent,
    destroy: Destroy,
    _lib: Library,
}
impl Mpv {
    fn open() -> Result<Self> {
        let mut paths: Vec<PathBuf> = vec![];
        if let Some(p) = std::env::var_os("YMTUI_MPV_PATH") {
            let p = PathBuf::from(p);
            if p.is_file() {
                paths.push(p);
            } else {
                for n in library_names() {
                    paths.push(p.join(n));
                    paths.push(p.join("bin").join(n));
                }
            }
        }
        for n in library_names() {
            paths.push(n.into());
        }
        if cfg!(windows) {
            for p in [r"C:\Program Files (x86)\mpv", r"C:\Program Files\mpv"] {
                for n in library_names() {
                    paths.push(PathBuf::from(p).join(n));
                }
            }
        }
        // SAFETY: all symbols use the documented libmpv C ABI. The Library stays
        // alive until after mpv_terminate_destroy; only the UI thread calls it.
        unsafe {
            let lib = paths
                .iter()
                .find_map(|p| Library::new(p).ok())
                .context("libmpv не найдена. Укажите папку в YMTUI_MPV_PATH")?;
            let create: Create = *lib.get(b"mpv_create\0")?;
            let initialize: Initialize = *lib.get(b"mpv_initialize\0")?;
            let option: OptionString = *lib.get(b"mpv_set_option_string\0")?;
            let command = *lib.get(b"mpv_command\0")?;
            let get = *lib.get(b"mpv_get_property\0")?;
            let set = *lib.get(b"mpv_set_property\0")?;
            let wait = *lib.get(b"mpv_wait_event\0")?;
            let destroy: Destroy = *lib.get(b"mpv_terminate_destroy\0")?;
            let handle = create();
            if handle.is_null() {
                bail!("mpv_create завершился ошибкой");
            }
            for (key, value) in [
                ("video", "no"),
                ("terminal", "no"),
                ("audio-display", "no"),
                ("idle", "yes"),
            ] {
                option(
                    handle,
                    CString::new(key)?.as_ptr(),
                    CString::new(value)?.as_ptr(),
                );
            }
            if initialize(handle) < 0 {
                destroy(handle);
                bail!("Не удалось инициализировать libmpv");
            }
            Ok(Self {
                handle,
                command,
                get,
                set,
                wait,
                destroy,
                _lib: lib,
            })
        }
    }
    fn command(&self, args: &[&str]) -> Result<()> {
        let strings: Vec<_> = args
            .iter()
            .map(|s| CString::new(*s))
            .collect::<std::result::Result<_, _>>()?;
        let mut pointers: Vec<_> = strings.iter().map(|s| s.as_ptr()).collect();
        pointers.push(std::ptr::null());
        let result = unsafe { (self.command)(self.handle, pointers.as_ptr()) };
        if result < 0 {
            bail!("Ошибка команды libmpv ({result})");
        }
        Ok(())
    }
    fn number(&self, name: &str) -> Option<f64> {
        let mut value = 0.0_f64;
        let result = unsafe {
            (self.get)(
                self.handle,
                CString::new(name).ok()?.as_ptr(),
                5,
                (&mut value as *mut f64).cast(),
            )
        };
        (result >= 0).then_some(value)
    }
    fn set_number(&self, name: &str, mut value: f64) -> Result<()> {
        let result = unsafe {
            (self.set)(
                self.handle,
                CString::new(name)?.as_ptr(),
                5,
                (&mut value as *mut f64).cast(),
            )
        };
        if result < 0 {
            bail!("Не удалось изменить {name}");
        }
        Ok(())
    }
}
impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.handle);
        }
    }
}
fn library_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["mpv-2.dll", "libmpv-2.dll", "mpv-1.dll"]
    } else if cfg!(target_os = "macos") {
        &["libmpv.2.dylib", "libmpv.dylib"]
    } else {
        &["libmpv.so.2", "libmpv.so.1", "libmpv.so"]
    }
}
impl Audio {
    pub fn new(demo: bool, volume: f64) -> Result<Self> {
        let mut audio = Self {
            mpv: if demo { None } else { Some(Mpv::open()?) },
            paused: false,
            playing: false,
            volume: 70.0,
            demo_position: 0.0,
            tick: Instant::now(),
            duration: 0.0,
        };
        audio.set_volume(volume)?;
        Ok(audio)
    }
    pub fn play(&mut self, url: &str, duration: f64) -> Result<()> {
        if let Some(mpv) = &self.mpv {
            mpv.command(&["loadfile", url, "replace"])?;
            mpv.command(&["set", "pause", "no"])?;
        }
        self.playing = true;
        self.paused = false;
        self.demo_position = 0.0;
        self.duration = duration;
        self.tick = Instant::now();
        Ok(())
    }
    pub fn stop(&mut self) -> Result<()> {
        if let Some(m) = &self.mpv {
            m.command(&["stop"])?;
        }
        self.playing = false;
        self.paused = false;
        Ok(())
    }
    pub fn pause(&mut self, value: bool) -> Result<()> {
        if let Some(m) = &self.mpv {
            m.command(&["set", "pause", if value { "yes" } else { "no" }])?;
        }
        self.paused = value;
        self.tick = Instant::now();
        Ok(())
    }
    pub fn position(&self) -> f64 {
        self.mpv
            .as_ref()
            .and_then(|m| m.number("time-pos"))
            .unwrap_or(self.demo_position)
    }
    pub fn seek(&mut self, value: f64) -> Result<()> {
        let value = value.clamp(0.0, self.duration.max(0.0));
        if let Some(m) = &self.mpv {
            m.command(&["seek", &value.to_string(), "absolute"])?;
        }
        self.demo_position = value;
        Ok(())
    }
    pub fn set_volume(&mut self, volume: f64) -> Result<()> {
        self.volume = volume.clamp(0.0, 100.0);
        if let Some(m) = &self.mpv {
            m.set_number("volume", self.volume)?;
        }
        Ok(())
    }
    /// Returns true once on natural EOF. Playback errors are never treated as EOF.
    pub fn tick(&mut self) -> Result<bool> {
        let elapsed = self.tick.elapsed().as_secs_f64();
        self.tick = Instant::now();
        if let Some(m) = &self.mpv {
            for _ in 0..100 {
                let event = unsafe { (m.wait)(m.handle, 0.0) };
                if event.is_null() {
                    break;
                }
                let event = unsafe { &*event };
                if event.event_id == 0 {
                    break;
                }
                if event.event_id == 7 && !event.data.is_null() {
                    let end = unsafe { &*event.data.cast::<EndFile>() };
                    if end.reason == 4 {
                        self.playing = false;
                        bail!("libmpv: ошибка воспроизведения ({})", end.error);
                    }
                    if end.reason == 0 && self.playing {
                        self.playing = false;
                        return Ok(true);
                    }
                }
            }
        } else if self.playing && !self.paused {
            self.demo_position += elapsed;
            if self.duration > 0.0 && self.demo_position >= self.duration {
                self.playing = false;
                return Ok(true);
            }
        }
        Ok(false)
    }
}
