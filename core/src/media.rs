//! What a message's attachment is, and how to fetch a picture of it.
//!
//! Only a screen-sized thumbnail is ever downloaded for display: full-size
//! originals would cost memory and traffic for no visible gain.

use std::path::Path;

use grammers_client::Client;
use grammers_client::media::{Downloadable, Media};
use grammers_client::message::Message;
use grammers_client::tl;
use grammers_session::types::PeerRef;

use crate::error::UiError;

/// Longest side, in pixels, of the thumbnail picked for display.
const MAX_THUMB_SIDE: i32 = 800;
/// Stickers have no meaningful size when the attribute is missing.
const STICKER_SIDE: i32 = 512;

/// A downloadable picture: a photo thumbnail, a static sticker or an avatar.
pub struct ImageSource {
    location: tl::enums::InputFileLocation,
    dc_id: Option<i32>,
    pub width: i32,
    pub height: i32,
    /// File extension matching the image format.
    pub ext: &'static str,
}

impl Downloadable for ImageSource {
    fn to_raw_input_location(&self) -> Option<tl::enums::InputFileLocation> {
        Some(self.location.clone())
    }

    fn dc_id(&self) -> Option<i32> {
        self.dc_id
    }
}

pub struct MediaInfo {
    /// "text" for a plain message, otherwise the attachment type.
    pub kind: &'static str,
    /// Extension for the full file when the client can play or show it.
    pub file_ext: Option<&'static str>,
    pub image: Option<ImageSource>,
    pub file_name: String,
    pub file_size: i64,
    /// Seconds of audio or video; 0 when not known.
    pub duration: i32,
    /// Voice messages: the waveform, 5 bits per bar, packed little-endian.
    pub waveform: Vec<u8>,
    /// Audio files: "performer — title" when the tags carry them.
    pub track: String,
}

impl MediaInfo {
    fn plain(kind: &'static str) -> Self {
        Self {
            kind,
            file_ext: None,
            image: None,
            file_name: String::new(),
            file_size: 0,
            duration: 0,
            waveform: Vec::new(),
            track: String::new(),
        }
    }
}

/// The raw media of a message.
fn raw_media(message: &Message) -> Option<tl::enums::MessageMedia> {
    match &message.raw {
        tl::enums::Message::Message(m) => m.media.clone(),
        _ => None,
    }
}

/// The media to show and download: grammers' own, or the photo / video
/// inside paid media when Telegram sent it (free or already bought).
pub fn media_of(message: &Message) -> Option<Media> {
    if let Some(media) = message.media() {
        return Some(media);
    }
    match raw_media(message)? {
        tl::enums::MessageMedia::PaidMedia(paid) => paid.extended_media.into_iter().find_map(|extended| match extended {
            tl::enums::MessageExtendedMedia::Media(inner) => Media::from_raw(inner.media),
            tl::enums::MessageExtendedMedia::Preview(_) => None,
        }),
        _ => None,
    }
}

/// What a message whose media grammers does not know holds: "paid",
/// "story", "giveaway", "invoice", "game", "todo", "unsupported"; `None`
/// when it holds no media at all.
fn other_kind(message: &Message) -> Option<&'static str> {
    use tl::enums::MessageMedia as M;
    Some(match raw_media(message)? {
        M::Empty | M::WebPage(_) => return None,
        M::PaidMedia(_) => "paid",
        M::Story(_) => "story",
        M::Giveaway(_) | M::GiveawayResults(_) => "giveaway",
        M::Invoice(_) => "invoice",
        M::Game(_) => "game",
        M::ToDo(_) => "todo",
        _ => "unsupported",
    })
}

pub fn describe(message: &Message) -> MediaInfo {
    if message.action().is_some() {
        return MediaInfo::plain("service");
    }
    let media = media_of(message);
    if media.is_none()
        && let Some(kind) = other_kind(message)
    {
        return MediaInfo::plain(kind);
    }
    match media {
        // A link preview is still an ordinary text message.
        None | Some(Media::WebPage(_)) => MediaInfo::plain("text"),
        Some(Media::Photo(photo)) => MediaInfo {
            file_ext: photo.raw.ttl_seconds.is_none().then_some("jpg"),
            image: photo_image(&photo.raw),
            ..MediaInfo::plain("photo")
        },
        Some(Media::Document(document)) => document_info(&document.raw),
        Some(Media::Sticker(sticker)) => document_info(&sticker.document.raw),
        Some(Media::Contact(_)) => MediaInfo::plain("contact"),
        Some(Media::Poll(_)) => MediaInfo::plain("poll"),
        Some(Media::Geo(_) | Media::GeoLive(_) | Media::Venue(_)) => MediaInfo::plain("location"),
        Some(Media::Dice(_)) => MediaInfo::plain("dice"),
        Some(_) => MediaInfo::plain("media"),
    }
}

/// Self-destructing ("view once" or timed) media, which is never stored.
pub fn is_ephemeral(media: &Media) -> bool {
    match media {
        Media::Photo(photo) => photo.raw.ttl_seconds.is_some(),
        Media::Document(document) => document.raw.ttl_seconds.is_some(),
        Media::Sticker(sticker) => sticker.document.raw.ttl_seconds.is_some(),
        _ => false,
    }
}

/// The profile photo of a peer, in its small variant.
pub fn avatar(peer: PeerRef, photo_id: i64) -> ImageSource {
    ImageSource {
        location: tl::types::InputPeerPhotoFileLocation {
            big: false,
            peer: peer.into(),
            photo_id,
        }
        .into(),
        dc_id: None,
        width: 160,
        height: 160,
        ext: "jpg",
    }
}

/// The still picture of a custom emoji: the emoji itself when it is a plain
/// image, its preview frame when it is animated.
pub fn custom_emoji(document: &tl::types::Document) -> Option<ImageSource> {
    let location = |thumb_size: String| -> tl::enums::InputFileLocation {
        tl::types::InputDocumentFileLocation {
            id: document.id,
            access_hash: document.access_hash,
            file_reference: document.file_reference.clone(),
            thumb_size,
        }
        .into()
    };
    if document.mime_type == "image/webp" {
        return Some(ImageSource {
            location: location(String::new()),
            dc_id: Some(document.dc_id),
            width: 100,
            height: 100,
            ext: "webp",
        });
    }
    let (thumb_size, width, height) = pick_thumb(document.thumbs.as_deref()?)?;
    Some(ImageSource {
        location: location(thumb_size),
        dc_id: Some(document.dc_id),
        width,
        height,
        // Preview frames of emoji are WebP whatever their size type says.
        ext: "webp",
    })
}

/// Downloads `source` to `path`. The file appears under its final name only
/// once complete, so a half-written picture is never served.
pub async fn fetch(client: &Client, source: &ImageSource, path: &Path) -> Result<(), UiError> {
    let mut bytes = Vec::new();
    let mut download = client.iter_download(source);
    while let Some(chunk) = download.next().await? {
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(UiError::new("MEDIA_EMPTY", "the picture is not available"));
    }

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("part");
    // Sealed while a passcode is set (see vault::read_media).
    std::fs::write(&partial, crate::vault::seal_media(bytes))?;
    std::fs::rename(&partial, path)?;
    Ok(())
}

fn photo_image(media: &tl::types::MessageMediaPhoto) -> Option<ImageSource> {
    // Self-destructing photos are not cached to disk.
    if media.ttl_seconds.is_some() {
        return None;
    }
    let Some(tl::enums::Photo::Photo(photo)) = &media.photo else {
        return None;
    };
    let (thumb_size, width, height) = pick_thumb(&photo.sizes)?;
    Some(ImageSource {
        location: tl::types::InputPhotoFileLocation {
            id: photo.id,
            access_hash: photo.access_hash,
            file_reference: photo.file_reference.clone(),
            thumb_size,
        }
        .into(),
        dc_id: Some(photo.dc_id),
        width,
        height,
        ext: "jpg",
    })
}

fn document_info(media: &tl::types::MessageMediaDocument) -> MediaInfo {
    let Some(tl::enums::Document::Document(document)) = &media.document else {
        return MediaInfo::plain("document");
    };

    let mut is_sticker = false;
    let mut is_animation = false;
    let mut is_voice = false;
    let mut is_round = false;
    let mut image_size = None;
    let mut file_name = String::new();
    let mut duration = 0;
    let mut waveform = Vec::new();
    let mut track = String::new();
    for attribute in &document.attributes {
        use tl::enums::DocumentAttribute as A;
        match attribute {
            A::Sticker(_) => is_sticker = true,
            A::Animated => is_animation = true,
            A::Audio(audio) => {
                is_voice = audio.voice;
                duration = audio.duration;
                waveform = audio.waveform.clone().unwrap_or_default();
                track = match (audio.performer.as_deref(), audio.title.as_deref()) {
                    (Some(p), Some(t)) if !p.is_empty() && !t.is_empty() => format!("{p} — {t}"),
                    (_, Some(t)) if !t.is_empty() => t.to_owned(),
                    (Some(p), _) if !p.is_empty() => p.to_owned(),
                    _ => String::new(),
                };
            }
            A::Video(video) => {
                is_round = video.round_message;
                duration = video.duration.round() as i32;
            }
            A::ImageSize(size) => image_size = Some((size.w, size.h)),
            A::Filename(name) => file_name = name.file_name.clone(),
            _ => {}
        }
    }

    let mime = document.mime_type.as_str();
    let kind = if is_sticker {
        "sticker"
    } else if is_round {
        "round"
    } else if mime.starts_with("video/") {
        if is_animation { "gif" } else { "video" }
    } else if is_voice {
        "voice"
    } else if mime.starts_with("audio/") {
        "audio"
    } else {
        "document"
    };

    let location = |thumb_size: String| -> tl::enums::InputFileLocation {
        tl::types::InputDocumentFileLocation {
            id: document.id,
            access_hash: document.access_hash,
            file_reference: document.file_reference.clone(),
            thumb_size,
        }
        .into()
    };
    let image = if media.ttl_seconds.is_some() {
        None
    } else if is_sticker && mime == "image/webp" {
        // A static sticker is itself a small image: show the real thing.
        let (width, height) = image_size.unwrap_or((STICKER_SIDE, STICKER_SIDE));
        Some(ImageSource {
            location: location(String::new()),
            dc_id: Some(document.dc_id),
            width,
            height,
            ext: "webp",
        })
    } else {
        // Videos, animated stickers and files show their preview frame.
        document
            .thumbs
            .as_deref()
            .and_then(pick_thumb)
            .map(|(thumb_size, width, height)| ImageSource {
                location: location(thumb_size),
                dc_id: Some(document.dc_id),
                width,
                height,
                ext: "jpg",
            })
    };

    // Formats the WebView can play. Anything else stays a plain file row.
    let file_ext = match (kind, mime) {
        _ if media.ttl_seconds.is_some() => None,
        ("video" | "gif" | "round", "video/mp4") => Some("mp4"),
        ("video" | "gif", "video/webm") => Some("webm"),
        // Video stickers loop like GIFs; animated (Lottie) ones stay a frame.
        ("sticker", "video/webm") => Some("webm"),
        ("voice" | "audio", "audio/ogg") => Some("ogg"),
        ("audio", "audio/mpeg") => Some("mp3"),
        ("audio", "audio/mp4" | "audio/x-m4a") => Some("m4a"),
        _ => None,
    };

    MediaInfo {
        kind,
        file_ext,
        image,
        file_name,
        file_size: document.size,
        duration,
        waveform,
        track,
    }
}

/// Picks the largest thumbnail that still fits `MAX_THUMB_SIDE`, or the
/// smallest one when all are bigger. Returns its type and dimensions.
fn pick_thumb(sizes: &[tl::enums::PhotoSize]) -> Option<(String, i32, i32)> {
    let downloadable = sizes.iter().filter_map(|size| match size {
        tl::enums::PhotoSize::Size(s) => Some((s.r#type.clone(), s.w, s.h)),
        tl::enums::PhotoSize::Progressive(s) => Some((s.r#type.clone(), s.w, s.h)),
        _ => None,
    });
    let (mut fitting, mut smallest): (Option<(String, i32, i32)>, Option<(String, i32, i32)>) =
        (None, None);
    for candidate in downloadable {
        let side = candidate.1.max(candidate.2);
        if side <= MAX_THUMB_SIDE && fitting.as_ref().is_none_or(|f| side > f.1.max(f.2)) {
            fitting = Some(candidate.clone());
        }
        if smallest.as_ref().is_none_or(|s| side < s.1.max(s.2)) {
            smallest = Some(candidate);
        }
    }
    fitting.or(smallest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(kind: &str, w: i32, h: i32) -> tl::enums::PhotoSize {
        tl::types::PhotoSize {
            r#type: kind.to_owned(),
            w,
            h,
            size: w * h / 10,
        }
        .into()
    }

    #[test]
    fn picks_the_largest_thumbnail_that_fits() {
        let stripped: tl::enums::PhotoSize = tl::types::PhotoStrippedSize {
            r#type: "i".to_owned(),
            bytes: vec![1, 2, 3],
        }
        .into();
        let sizes = [
            stripped.clone(),
            size("m", 320, 240),
            size("x", 800, 600),
            size("y", 1280, 960),
        ];
        assert_eq!(pick_thumb(&sizes), Some(("x".to_owned(), 800, 600)));

        // Nothing fits: take the smallest rather than the multi-megabyte one.
        let big = [size("y", 1280, 960), size("w", 2560, 1920)];
        assert_eq!(pick_thumb(&big), Some(("y".to_owned(), 1280, 960)));

        // Inline-only previews cannot be requested from the server.
        assert_eq!(pick_thumb(&[stripped]), None);
    }
}
