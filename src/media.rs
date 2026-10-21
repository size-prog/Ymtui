use crate::{audio::Audio, model::Track};
use anyhow::{Result, anyhow};
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};
use std::{
    sync::mpsc::{self, Receiver},
    time::Duration,
};

pub struct Media {
    controls: MediaControls,
    rx: Receiver<MediaControlEvent>,
    #[cfg(windows)]
    window: windows_sys::Win32::Foundation::HWND,
}
impl Media {
    pub fn new() -> Result<Self> {
        #[cfg(windows)]
        let window = unsafe {
            use windows_sys::Win32::{
                System::LibraryLoader::GetModuleHandleW,
                UI::WindowsAndMessaging::{CreateWindowExW, WS_OVERLAPPEDWINDOW},
            };
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let title: Vec<u16> = "YaMusic Media\0".encode_utf16().collect();
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            );
            if hwnd.is_null() {
                return Err(anyhow!("Не удалось создать окно медиасессии"));
            }
            hwnd
        };
        #[cfg(windows)]
        let hwnd = Some(window.cast());
        #[cfg(not(windows))]
        let hwnd = None;
        let result = (|| {
            let mut controls = MediaControls::new(PlatformConfig {
                dbus_name: "ymtui",
                display_name: "YaMusic",
                hwnd,
            })
            .map_err(|e| anyhow!("{e:?}"))?;
            let (tx, rx) = mpsc::channel();
            controls
                .attach(move |event| {
                    let _ = tx.send(event);
                })
                .map_err(|e| anyhow!("{e:?}"))?;
            Ok(Self {
                controls,
                rx,
                #[cfg(windows)]
                window,
            })
        })();
        #[cfg(windows)]
        if result.is_err() {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(window);
            }
        }
        result
    }
    pub fn commands(&self) -> Vec<MediaControlEvent> {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            let mut msg = std::mem::zeroed();
            for _ in 0..100 {
                if PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) == 0 {
                    break;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        self.rx.try_iter().take(100).collect()
    }
    pub fn publish(&mut self, track: Option<&Track>, audio: &Audio, changed: bool) -> Result<()> {
        if changed && let Some(t) = track {
            let artist = t.artist();
            self.controls
                .set_metadata(MediaMetadata {
                    title: Some(&t.title),
                    album: Some(&t.album.title),
                    artist: Some(&artist),
                    cover_url: (!t.cover.is_empty()).then_some(t.cover.as_str()),
                    duration: Some(Duration::from_secs_f64(t.duration())),
                })
                .map_err(|e| anyhow!("{e:?}"))?;
        }
        let progress = Some(MediaPosition(Duration::from_secs_f64(
            audio.position().max(0.0),
        )));
        self.controls
            .set_playback(if !audio.playing {
                MediaPlayback::Stopped
            } else if audio.paused {
                MediaPlayback::Paused { progress }
            } else {
                MediaPlayback::Playing { progress }
            })
            .map_err(|e| anyhow!("{e:?}"))?;
        Ok(())
    }
}
impl Drop for Media {
    fn drop(&mut self) {
        let _ = self.controls.set_playback(MediaPlayback::Stopped);
        let _ = self.controls.detach();
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(self.window);
        }
    }
}
