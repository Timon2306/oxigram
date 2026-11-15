//! Video on Linux through GStreamer: a `playbin` whose video sink is an
//! `appsink` that hands out RGBA frames. Nothing runs on our own threads;
//! GStreamer's streaming threads decode, the UI thread pulls.

use std::cell::{Cell, RefCell};
use std::path::Path;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

use super::{Frame, Options, Player};

/// Largest size that fits `max_w` x `max_h` with the aspect ratio of
/// `w` x `h`; never upscales. Zero in, zero out. (The pipeline gets the same
/// result from caps ranges; this documents and tests the rule.)
#[cfg_attr(not(test), allow(dead_code))]
fn fit_size(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if w == 0 || h == 0 || max_w == 0 || max_h == 0 {
        return (0, 0);
    }
    if w <= max_w && h <= max_h {
        return (w, h);
    }
    if (w as u64) * (max_h as u64) >= (h as u64) * (max_w as u64) {
        let nh = ((h as u64 * max_w as u64) / w as u64).max(1) as u32;
        (max_w, nh)
    } else {
        let nw = ((w as u64 * max_h as u64) / h as u64).max(1) as u32;
        (nw, max_h)
    }
}

/// Copies `height` rows of `row_bytes` out of `src` whose rows are `stride`
/// bytes apart, into a tightly packed buffer. None if `src` is too short.
fn pack_rows(src: &[u8], stride: usize, row_bytes: usize, height: usize) -> Option<Vec<u8>> {
    if row_bytes == 0 || height == 0 || stride < row_bytes {
        return None;
    }
    let needed = stride.checked_mul(height - 1)?.checked_add(row_bytes)?;
    if src.len() < needed {
        return None;
    }
    if stride == row_bytes {
        return Some(src[..row_bytes * height].to_vec());
    }
    let mut out = Vec::with_capacity(row_bytes * height);
    for row in 0..height {
        let start = row * stride;
        out.extend_from_slice(&src[start..start + row_bytes]);
    }
    Some(out)
}

struct GstPlayer {
    pipeline: gst::Element,
    sink: gst_app::AppSink,
    bus: gst::Bus,
    looped: bool,
    paused: Cell<bool>,
    ended: Cell<bool>,
    error: RefCell<Option<String>>,
    /// Bumped on every seek so a paused preroll is delivered again.
    epoch: Cell<u32>,
    /// (epoch, pts) of the last frame handed out from the preroll.
    last_preroll: Cell<Option<(u32, Option<u64>)>>,
}

pub fn open(path: &Path, options: Options) -> Result<Box<dyn Player>, String> {
    gst::init().map_err(|e| format!("GStreamer init failed: {e}"))?;

    let missing = |name: &str| {
        format!("GStreamer element \"{name}\" is missing (install gst-plugins-base / gst-plugins-good)")
    };
    let make = |factory: &str| {
        gst::ElementFactory::make(factory)
            .build()
            .map_err(|_| missing(factory))
    };

    let abs = path
        .canonicalize()
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let uri = gst::glib::filename_to_uri(&abs, None).map_err(|e| format!("bad file path: {e}"))?;

    let playbin = make("playbin")?;

    let max_w = options.max_width.clamp(1, i32::MAX as u32) as i32;
    let max_h = options.max_height.clamp(1, i32::MAX as u32) as i32;
    // Ranges: videoscale picks the size closest to the source that fits,
    // keeping the aspect ratio, and never enlarges.
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", "RGBA")
        .field("width", gst::IntRange::new(1, max_w))
        .field("height", gst::IntRange::new(1, max_h))
        .build();
    let sink = gst_app::AppSink::builder()
        .caps(&caps)
        .sync(true)
        .max_buffers(1)
        .drop(true)
        .build();

    let scale = make("videoscale")?;
    let convert = make("videoconvert")?;
    let filter = gst::ElementFactory::make("capsfilter")
        .property("caps", &caps)
        .build()
        .map_err(|_| missing("capsfilter"))?;

    let bin = gst::Bin::new();
    let sink_el: &gst::Element = sink.upcast_ref();
    bin.add_many([&scale, &convert, &filter, sink_el])
        .map_err(|e| e.to_string())?;
    gst::Element::link_many([&scale, &convert, &filter, sink_el])
        .map_err(|e| format!("cannot link the video sink: {e}"))?;
    let pad = scale
        .static_pad("sink")
        .ok_or_else(|| "videoscale has no sink pad".to_owned())?;
    let ghost = gst::GhostPad::with_target(&pad).map_err(|e| e.to_string())?;
    bin.add_pad(&ghost).map_err(|e| e.to_string())?;

    playbin.set_property("uri", uri.as_str());
    playbin.set_property("video-sink", &bin);
    playbin.set_property("mute", options.muted);

    let bus = playbin.bus().ok_or_else(|| "pipeline has no bus".to_owned())?;
    let want = if options.autoplay {
        gst::State::Playing
    } else {
        gst::State::Paused
    };
    if playbin.set_state(want).is_err() {
        let _ = playbin.set_state(gst::State::Null);
        return Err("cannot start the GStreamer pipeline".to_owned());
    }

    Ok(Box::new(GstPlayer {
        pipeline: playbin,
        sink,
        bus,
        looped: options.looped,
        paused: Cell::new(!options.autoplay),
        ended: Cell::new(false),
        error: RefCell::new(None),
        epoch: Cell::new(0),
        last_preroll: Cell::new(None),
    }))
}

impl GstPlayer {
    /// Drains pending bus messages without blocking.
    fn poll_bus(&self) {
        while let Some(msg) = self
            .bus
            .pop_filtered(&[gst::MessageType::Eos, gst::MessageType::Error])
        {
            match msg.view() {
                gst::MessageView::Eos(_) => {
                    if self.looped {
                        self.seek_to(0.0);
                    } else {
                        self.ended.set(true);
                    }
                }
                gst::MessageView::Error(e) => {
                    let mut text = e.error().to_string();
                    if let Some(debug) = e.debug() {
                        text.push_str(&format!(" ({debug})"));
                    }
                    *self.error.borrow_mut() = Some(text);
                }
                _ => {}
            }
        }
    }

    fn seek_to(&self, seconds: f64) {
        let ns = (seconds.max(0.0) * 1e9) as u64;
        let ok = self
            .pipeline
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_nseconds(ns),
            )
            .is_ok();
        if ok {
            self.epoch.set(self.epoch.get().wrapping_add(1));
            self.ended.set(false);
        }
    }

    fn to_frame(sample: &gst::Sample) -> Option<Frame> {
        let info = gst_video::VideoInfo::from_caps(sample.caps()?).ok()?;
        let buffer = sample.buffer()?;
        let map = buffer.map_readable().ok()?;
        let (w, h) = (info.width() as usize, info.height() as usize);
        let stride = usize::try_from(*info.stride().first()?).ok()?;
        let rgba = pack_rows(map.as_slice(), stride, w * 4, h)?;
        Some(Frame {
            width: w as u32,
            height: h as u32,
            rgba,
        })
    }
}

impl Player for GstPlayer {
    fn frame(&mut self) -> Option<Frame> {
        self.poll_bus();
        if let Some(sample) = self.sink.try_pull_sample(gst::ClockTime::ZERO) {
            return Self::to_frame(&sample);
        }
        // Paused (or just sought while paused): the picture is the preroll
        // sample, which stays available, so hand it out once per seek.
        if self.paused.get() {
            if let Some(sample) = self.sink.try_pull_preroll(gst::ClockTime::ZERO) {
                let pts = sample.buffer().and_then(|b| b.pts()).map(|t| t.nseconds());
                let key = (self.epoch.get(), pts);
                if self.last_preroll.get() != Some(key) {
                    self.last_preroll.set(Some(key));
                    return Self::to_frame(&sample);
                }
            }
        }
        None
    }

    fn play(&mut self) {
        if self.ended.get() {
            self.seek_to(0.0);
        }
        self.paused.set(false);
        let _ = self.pipeline.set_state(gst::State::Playing);
    }

    fn pause(&mut self) {
        self.paused.set(true);
        let _ = self.pipeline.set_state(gst::State::Paused);
    }

    fn is_paused(&self) -> bool {
        self.paused.get()
    }

    fn seek(&mut self, seconds: f64) {
        self.seek_to(seconds);
    }

    fn position(&self) -> f64 {
        self.pipeline
            .query_position::<gst::ClockTime>()
            .map(|t| t.nseconds() as f64 / 1e9)
            .unwrap_or(0.0)
    }

    fn duration(&self) -> f64 {
        self.pipeline
            .query_duration::<gst::ClockTime>()
            .map(|t| t.nseconds() as f64 / 1e9)
            .unwrap_or(0.0)
    }

    fn set_volume(&mut self, volume: f32) {
        self.pipeline
            .set_property("volume", volume.clamp(0.0, 1.0) as f64);
    }

    fn set_muted(&mut self, muted: bool) {
        self.pipeline.set_property("mute", muted);
    }

    fn ended(&self) -> bool {
        self.poll_bus();
        self.ended.get()
    }

    fn error(&self) -> Option<String> {
        self.poll_bus();
        self.error.borrow().clone()
    }
}

impl Drop for GstPlayer {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect() {
        assert_eq!(fit_size(1920, 1080, 960, 960), (960, 540));
        assert_eq!(fit_size(1080, 1920, 960, 960), (540, 960));
        assert_eq!(fit_size(100, 50, 960, 960), (100, 50));
        assert_eq!(fit_size(0, 50, 960, 960), (0, 0));
        assert_eq!(fit_size(4000, 1, 100, 100), (100, 1));
    }

    #[test]
    fn pack_strips_padding() {
        let mut src = Vec::new();
        for r in 0..3u8 {
            src.extend_from_slice(&[r; 8]);
            src.extend_from_slice(&[0xEE; 4]);
        }
        let out = pack_rows(&src, 12, 8, 3).unwrap();
        assert_eq!(out.len(), 24);
        assert_eq!(&out[8..16], &[1u8; 8]);
        assert!(!out.contains(&0xEE));
    }

    #[test]
    fn pack_tight_and_short() {
        let src = vec![7u8; 16];
        assert_eq!(pack_rows(&src, 8, 8, 2).unwrap(), src);
        assert!(pack_rows(&src[..15], 8, 8, 2).is_none());
        assert!(pack_rows(&src[..14], 8, 6, 2).is_some());
    }
}
