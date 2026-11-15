//! Video on macOS: AVPlayer decodes (hardware where it can), an
//! `AVPlayerItemVideoOutput` hands over BGRA pixel buffers, and `frame()`
//! copies the newest one into an RGBA `Frame`, scaled down to the requested
//! size. Everything runs on the main thread and nothing blocks: the item loads
//! asynchronously, so duration/errors simply become available later.

use std::path::Path;
use std::ptr;

use objc2::{AnyThread, MainThreadMarker};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVPlayer, AVPlayerActionAtItemEnd, AVPlayerItem, AVPlayerItemStatus, AVPlayerItemVideoOutput,
};
use objc2_core_foundation::CFString;
use objc2_core_media::CMTime;
use objc2_core_video::{
    CVGetCurrentHostTime, CVGetHostClockFrequency, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use super::{Frame, Options, Player};

/// How close to the end counts as "the end", in seconds.
const END_EPSILON: f64 = 0.05;

struct MacPlayer {
    player: Retained<AVPlayer>,
    item: Retained<AVPlayerItem>,
    output: Retained<AVPlayerItemVideoOutput>,
    looped: bool,
    max_width: u32,
    max_height: u32,
    /// What the user asked for; `rate()` drops to 0 at the end by itself.
    paused: bool,
}

pub fn open(path: &Path, options: Options) -> Result<Box<dyn Player>, String> {
    let mtm = MainThreadMarker::new().ok_or("video must be opened on the main thread")?;
    let path = path
        .to_str()
        .ok_or_else(|| "the file path is not valid UTF-8".to_owned())?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));

    unsafe {
        // Ask for BGRA, the format that needs no conversion pass on the GPU
        // side and is cheap to swizzle on the CPU.
        let key = &*(kCVPixelBufferPixelFormatTypeKey as *const CFString as *const NSString);
        let format = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
        let value: &AnyObject = &format;
        let attributes = NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[value]);
        let output = AVPlayerItemVideoOutput::initWithPixelBufferAttributes(
            AVPlayerItemVideoOutput::alloc(),
            Some(&attributes),
        );

        let item = AVPlayerItem::playerItemWithURL(&url, mtm);
        item.addOutput(&output);

        let player = AVPlayer::playerWithPlayerItem(Some(&item), mtm);
        player.setActionAtItemEnd(AVPlayerActionAtItemEnd::Pause);
        player.setMuted(options.muted);

        let mut this = MacPlayer {
            player,
            item,
            output,
            looped: options.looped,
            max_width: options.max_width,
            max_height: options.max_height,
            paused: true,
        };
        if options.autoplay {
            this.play();
        }
        Ok(Box::new(this))
    }
}

fn seconds(time: CMTime) -> f64 {
    let s = unsafe { time.seconds() };
    if s.is_finite() && s > 0.0 { s } else { 0.0 }
}

fn host_seconds() -> f64 {
    CVGetCurrentHostTime() as f64 / CVGetHostClockFrequency()
}

impl MacPlayer {
    fn at_end(&self) -> bool {
        let duration = self.duration();
        duration > 0.0 && self.position() >= duration - END_EPSILON
    }

    fn seek_to_start(&self) {
        unsafe {
            let zero = CMTime::new(0, 1);
            self.player
                .seekToTime_toleranceBefore_toleranceAfter(zero, zero, zero);
        }
    }

    fn failed(&self) -> bool {
        unsafe { self.item.status() == AVPlayerItemStatus::Failed }
    }
}

impl Player for MacPlayer {
    fn frame(&mut self) -> Option<Frame> {
        if self.failed() {
            return None;
        }
        // At the end the player pauses itself; a loop starts over.
        if self.looped && !self.paused && self.at_end() {
            self.seek_to_start();
            unsafe { self.player.play() };
        }
        unsafe {
            // While playing, ask for the picture due on screen now; while
            // paused (or just seeked) for the one at the current position.
            let time = if self.player.rate() == 0.0 {
                self.item.currentTime()
            } else {
                self.output.itemTimeForHostTime(host_seconds())
            };
            if !self.output.hasNewPixelBufferForItemTime(time) {
                return None;
            }
            let buffer = self
                .output
                .copyPixelBufferForItemTime_itemTimeForDisplay(time, ptr::null_mut())?;
            if CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags::ReadOnly) != 0 {
                return None;
            }
            let width = CVPixelBufferGetWidth(&buffer);
            let height = CVPixelBufferGetHeight(&buffer);
            let stride = CVPixelBufferGetBytesPerRow(&buffer);
            let base = CVPixelBufferGetBaseAddress(&buffer) as *const u8;
            let frame = if base.is_null() || width == 0 || height == 0 || stride < width * 4 {
                None
            } else {
                let src = std::slice::from_raw_parts(base, stride * height);
                let (dw, dh) = fit_size(width as u32, height as u32, self.max_width, self.max_height);
                Some(Frame {
                    width: dw,
                    height: dh,
                    rgba: bgra_to_rgba(src, width, height, stride, dw as usize, dh as usize),
                })
            };
            CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags::ReadOnly);
            frame
        }
    }

    fn play(&mut self) {
        // Play after the end starts from the beginning.
        if !self.looped && self.at_end() {
            self.seek_to_start();
        }
        self.paused = false;
        unsafe { self.player.play() };
    }

    fn pause(&mut self) {
        self.paused = true;
        unsafe { self.player.pause() };
    }

    fn is_paused(&self) -> bool {
        self.paused
    }

    fn seek(&mut self, seconds: f64) {
        let target = seconds.max(0.0);
        let duration = self.duration();
        let target = if duration > 0.0 { target.min(duration) } else { target };
        unsafe {
            let time = CMTime::with_seconds(target, 600);
            let zero = CMTime::new(0, 1);
            self.player
                .seekToTime_toleranceBefore_toleranceAfter(time, zero, zero);
        }
    }

    fn position(&self) -> f64 {
        seconds(unsafe { self.item.currentTime() })
    }

    fn duration(&self) -> f64 {
        // Indefinite (not loaded yet, live) comes out as NaN: 0.
        seconds(unsafe { self.item.duration() })
    }

    fn set_volume(&mut self, volume: f32) {
        unsafe { self.player.setVolume(volume.clamp(0.0, 1.0)) };
    }

    fn set_muted(&mut self, muted: bool) {
        unsafe { self.player.setMuted(muted) };
    }

    fn ended(&self) -> bool {
        !self.looped && self.at_end()
    }

    fn error(&self) -> Option<String> {
        unsafe {
            if self.item.status() != AVPlayerItemStatus::Failed {
                return None;
            }
            Some(match self.item.error() {
                Some(error) => error.localizedDescription().to_string(),
                None => "the video cannot be played".to_owned(),
            })
        }
    }
}

impl Drop for MacPlayer {
    fn drop(&mut self) {
        unsafe {
            self.player.pause();
            self.item.removeOutput(&self.output);
        }
    }
}

/// `width` x `height` shrunk to fit in `max_width` x `max_height`, keeping the
/// aspect ratio; never enlarged, never smaller than 1 px. A zero maximum
/// means "no limit" on that side.
fn fit_size(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let mut scale = 1.0f64;
    if max_width > 0 && width > max_width {
        scale = scale.min(max_width as f64 / width as f64);
    }
    if max_height > 0 && height > max_height {
        scale = scale.min(max_height as f64 / height as f64);
    }
    if scale >= 1.0 {
        return (width.max(1), height.max(1));
    }
    let w = ((width as f64 * scale).round() as u32).clamp(1, max_width.max(1));
    let h = ((height as f64 * scale).round() as u32).clamp(1, max_height.max(1));
    (w, h)
}

/// Copies BGRA rows (`stride` bytes apart, maybe padded) into tightly packed
/// RGBA of `dw` x `dh`, sampling the nearest source pixel when it is smaller.
fn bgra_to_rgba(src: &[u8], sw: usize, sh: usize, stride: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut out = vec![0u8; dw * dh * 4];
    if dw == sw && dh == sh {
        for y in 0..dh {
            let row = &src[y * stride..y * stride + sw * 4];
            let dst = &mut out[y * dw * 4..(y + 1) * dw * 4];
            for (d, s) in dst.chunks_exact_mut(4).zip(row.chunks_exact(4)) {
                d[0] = s[2];
                d[1] = s[1];
                d[2] = s[0];
                d[3] = s[3];
            }
        }
        return out;
    }
    let columns: Vec<usize> = (0..dw).map(|x| (x * sw / dw).min(sw - 1) * 4).collect();
    for y in 0..dh {
        let sy = (y * sh / dh).min(sh - 1);
        let row = &src[sy * stride..sy * stride + sw * 4];
        let dst = &mut out[y * dw * 4..(y + 1) * dw * 4];
        for (d, &sx) in dst.chunks_exact_mut(4).zip(&columns) {
            d[0] = row[sx + 2];
            d[1] = row[sx + 1];
            d[2] = row[sx];
            d[3] = row[sx + 3];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_keeping_aspect() {
        assert_eq!(fit_size(1920, 1080, 960, 960), (960, 540));
        assert_eq!(fit_size(1080, 1920, 960, 960), (540, 960));
        assert_eq!(fit_size(640, 480, 1000, 1000), (640, 480));
        assert_eq!(fit_size(640, 480, 0, 0), (640, 480));
        assert_eq!(fit_size(4000, 10, 100, 100), (100, 1));
    }

    #[test]
    fn copies_rows_with_padding_and_swaps_channels() {
        // 2x2, stride 12 (4 bytes padding per row).
        let src = [
            1, 2, 3, 4, 5, 6, 7, 8, 99, 99, 99, 99, //
            9, 10, 11, 12, 13, 14, 15, 16, 99, 99, 99, 99,
        ];
        let out = bgra_to_rgba(&src, 2, 2, 12, 2, 2);
        assert_eq!(out, [3, 2, 1, 4, 7, 6, 5, 8, 11, 10, 9, 12, 15, 14, 13, 16]);
    }

    #[test]
    fn scales_down_by_sampling() {
        // 4x2 -> 2x1: takes pixels 0 and 2 of row 0.
        let mut src = Vec::new();
        for i in 0..8u8 {
            src.extend_from_slice(&[i, 0, 0, 255]);
        }
        let out = bgra_to_rgba(&src, 4, 2, 16, 2, 1);
        assert_eq!(out, [0, 0, 0, 255, 0, 0, 2, 255]);
    }
}
