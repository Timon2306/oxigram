//! Video playback inside the viewer, on each system's own media engine:
//! Media Foundation on Windows 8+, AVFoundation on macOS, GStreamer on Linux
//! (behind the `gstreamer` feature). Where none is available (Windows 7, a
//! Linux build without GStreamer) `open` fails and the caller falls back to
//! the system player.
//!
//! The UI pulls frames: a timer asks `frame()` about once per display frame
//! and gets a picture only when a new one was decoded.

use std::path::Path;

#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(all(target_os = "linux", feature = "gstreamer"))]
mod linux;

/// One decoded picture, RGBA, 8 bits per channel, rows tightly packed.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A playing file. Every method is called on the UI thread; implementations
/// keep their decoding on their own threads.
pub trait Player {
    /// The newest frame if it changed since the last call.
    fn frame(&mut self) -> Option<Frame>;
    fn play(&mut self);
    fn pause(&mut self);
    fn is_paused(&self) -> bool;
    /// Jumps to `seconds` from the start.
    fn seek(&mut self, seconds: f64);
    /// Seconds from the start.
    fn position(&self) -> f64;
    /// Seconds; 0 until known.
    fn duration(&self) -> f64;
    /// 0.0 ..= 1.0.
    fn set_volume(&mut self, volume: f32);
    fn set_muted(&mut self, muted: bool);
    /// Played to the end (and not looping).
    fn ended(&self) -> bool;
    /// The error that stopped playback, if any.
    fn error(&self) -> Option<String>;
}

pub struct Options {
    /// Start playing at once.
    pub autoplay: bool,
    /// Start again at the end (GIFs).
    pub looped: bool,
    pub muted: bool,
    /// Largest frame size wanted, in device pixels: bigger videos are scaled
    /// down by the engine so frames stay small in memory.
    pub max_width: u32,
    pub max_height: u32,
}

/// Opens `path` with the system's engine, or explains why it cannot.
pub fn open(path: &Path, options: Options) -> Result<Box<dyn Player>, String> {
    #[cfg(windows)]
    return windows::open(path, options);
    #[cfg(target_os = "macos")]
    return macos::open(path, options);
    #[cfg(all(target_os = "linux", feature = "gstreamer"))]
    return linux::open(path, options);
    #[allow(unreachable_code)]
    {
        let _ = (path, options);
        Err("no video engine in this build".to_owned())
    }
}
