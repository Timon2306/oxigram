//! Links in message text: bare URLs, hidden links (TextUrl), @mentions,
//! mentions by name, #hashtags and e-mail addresses.
//!
//! The bubble draws a message with links through Slint's `StyledText`, built
//! from markdown. Everything that is not a link is escaped, so the message's
//! own characters never turn into markup. Where a link points is encoded in
//! its target: `oxi-hidden:`, `oxi-mention:`, `oxi-user:` and `oxi-tag:` are
//! handled here; anything else is a plain URL for the browser.

use oxigram_core::actions;
use oxigram_core::tr;
use oxigram_core::store::chats::{Entity, MessageRow};

use crate::{format, run, tg, ui, with_app};

/// A link in a message, for "Копировать ссылку".
pub struct Link {
    /// What is copied: the URL, "@name" or "#tag".
    pub copy: String,
}

pub fn wire(ui: &crate::AppWindow) {
    ui.on_open_link(|target| open(target.as_str()));
    ui.on_open_reply(crate::replies::open);
}

/// The kinds of entities drawn as links.
fn is_link(kind: &str) -> bool {
    matches!(kind, "url" | "textUrl" | "mention" | "mentionName" | "hashtag" | "email")
}

/// The UTF-16 range of an entity as a byte range of `text`.
fn byte_range(text: &str, offset: i32, length: i32) -> Option<(usize, usize)> {
    let (start16, end16) = (offset.max(0) as usize, (offset + length).max(0) as usize);
    let (mut start, mut end) = (None, None);
    let mut pos16 = 0usize;
    for (byte, ch) in text.char_indices() {
        if pos16 == start16 {
            start = Some(byte);
        }
        if pos16 == end16 {
            end = Some(byte);
        }
        pos16 += ch.len_utf16();
    }
    if pos16 == start16 {
        start = Some(text.len());
    }
    if pos16 == end16 {
        end = Some(text.len());
    }
    match (start, end) {
        (Some(s), Some(e)) if s < e => Some((s, e)),
        _ => None,
    }
}

/// Where a link entity leads, as the StyledText target.
fn target(entity: &Entity, shown: &str) -> Option<String> {
    let shown = shown.trim();
    Some(match entity.kind.as_str() {
        "url" => with_scheme(shown),
        "textUrl" => {
            let url = entity.target.as_deref()?.trim();
            // A link whose text is its own address opens without asking.
            if same_address(url, shown) { with_scheme(url) } else { format!("oxi-hidden:{url}") }
        }
        "mention" => format!("oxi-mention:{}", shown.trim_start_matches('@')),
        "mentionName" => format!("oxi-user:{}", entity.target.as_deref()?),
        "hashtag" => format!("oxi-tag:{shown}"),
        "email" => format!("mailto:{shown}"),
        _ => return None,
    })
}

fn with_scheme(url: &str) -> String {
    let lower = url.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("mailto:") || lower.starts_with("tg:") {
        url.to_owned()
    } else {
        format!("https://{url}")
    }
}

/// "https://example.com/" and "example.com" are the same address.
fn same_address(a: &str, b: &str) -> bool {
    fn bare(s: &str) -> String {
        let s = s.trim().to_ascii_lowercase();
        let s = s.split_once("://").map_or(s.as_str(), |(_, rest)| rest).to_owned();
        let s = s.strip_prefix("www.").unwrap_or(&s).to_owned();
        s.trim_end_matches('/').to_owned()
    }
    bare(a) == bare(b)
}

/// The links of a message, in order, without overlaps.
fn spans<'a>(text: &str, entities: &'a [Entity]) -> Vec<(usize, usize, String, &'a Entity)> {
    let mut out: Vec<(usize, usize, String, &Entity)> = Vec::new();
    let mut sorted: Vec<&Entity> = entities.iter().filter(|e| is_link(&e.kind)).collect();
    sorted.sort_by_key(|e| e.offset);
    for entity in sorted {
        let Some((start, end)) = byte_range(text, entity.offset, entity.length) else { continue };
        if out.last().is_some_and(|(_, last_end, _, _)| start < *last_end) {
            continue;
        }
        if let Some(target) = target(entity, &text[start..end]) {
            out.push((start, end, target, entity));
        }
    }
    out
}

/// The links to offer in the message menu.
pub fn links(m: &MessageRow) -> Vec<Link> {
    let mut out: Vec<Link> = Vec::new();
    for (start, end, _, entity) in spans(&m.text, &m.entities) {
        let shown = m.text[start..end].trim().to_owned();
        let copy = match entity.kind.as_str() {
            "textUrl" => entity.target.clone().unwrap_or(shown),
            "url" => with_scheme(&shown),
            "mentionName" => continue,
            _ => shown,
        };
        if !out.iter().any(|l| l.copy == copy) {
            out.push(Link { copy });
        }
    }
    out
}

/// "Копировать ссылку" (`link-N` is the N-th link of the menu).
pub fn copy_link(id: i32, action: &str) {
    let Some(index) = action.strip_prefix("link-").and_then(|n| n.parse::<usize>().ok()) else { return };
    let Some(Some(row)) = with_app(|app| app.messages.iter().find(|m| m.id == id).cloned()) else { return };
    let Some(link) = links(&row).into_iter().nth(index) else { return };
    if let Some(ui) = ui() {
        ui.invoke_copy_text(link.copy.into());
    }
    crate::menus::toast(tr!("Link copied"));
}

/// The text with its links as markdown for `StyledText`, or `None` when the
/// message has no links (it is then drawn as plain text, which is cheaper).
pub fn markdown(text: &str, entities: &[Entity]) -> Option<String> {
    let spans = spans(text, entities);
    if spans.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(text.len() * 2);
    let mut at = 0;
    for (start, end, target, _) in &spans {
        escape_into(&mut out, &text[at..*start], at == 0 || text[..at].ends_with('\n'));
        out.push('[');
        escape_into(&mut out, &text[*start..*end], false);
        out.push_str("](<");
        out.push_str(&escape_target(target));
        out.push_str(">)");
        at = *end;
    }
    escape_into(&mut out, &text[at..], false);
    Some(out)
}

/// Markdown-safe text: every ASCII punctuation character escaped, leading
/// spaces kept (as no-break spaces), empty lines kept (as a zero-width space,
/// since a blank line would only end the paragraph).
fn escape_into(out: &mut String, text: &str, mut line_start: bool) {
    for ch in text.chars() {
        match ch {
            '\r' => {}
            '\n' => {
                if line_start {
                    out.push('\u{200B}');
                }
                out.push('\n');
                line_start = true;
                continue;
            }
            ' ' | '\t' if line_start => {
                // Still at the start of the line: the next space too.
                out.push('\u{00A0}');
                continue;
            }
            c if c.is_ascii_punctuation() => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
        line_start = false;
    }
}

/// A link target inside `<…>`: no line breaks or angle brackets.
fn escape_target(target: &str) -> String {
    let mut out = String::with_capacity(target.len());
    for ch in target.chars() {
        match ch {
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '\\' => out.push_str("%5C"),
            '\n' | '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// A click on a link.
fn open(target: &str) {
    if let Some(url) = target.strip_prefix("oxi-hidden:") {
        let shown = url.to_owned();
        let url = url.to_owned();
        crate::menus::confirm(&tr!("Open the link?"), &shown, &tr!("Open"), "", move |_| open_url(&url));
    } else if let Some(name) = target.strip_prefix("oxi-mention:") {
        open_username(name.to_owned(), None);
    } else if let Some(id) = target.strip_prefix("oxi-user:") {
        if let Ok(id) = id.parse::<i64>() {
            crate::open_chat(id);
        }
    } else if let Some(tag) = target.strip_prefix("oxi-tag:") {
        // As in Telegram Desktop: the hashtag is searched in this chat.
        if let Some(ui) = ui() {
            ui.invoke_search_in_chat(tag.into());
        }
    } else {
        open_url(target);
    }
}

/// t.me/name and tg://resolve?domain=name open the chat in Oxigram; other
/// addresses go to the browser.
pub fn open_url(url: &str) {
    if let Some(name) = telegram_username(url) {
        open_username(name, Some(url.to_owned()));
        return;
    }
    browse(url);
}

/// The @username a Telegram link points to.
fn telegram_username(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    let rest = lower.split_once("://").map_or(lower.as_str(), |(_, r)| r);
    if let Some(query) = rest.strip_prefix("resolve?") {
        return query.split('&').find_map(|p| p.strip_prefix("domain=")).map(str::to_owned);
    }
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let path = rest.strip_prefix("t.me/").or_else(|| rest.strip_prefix("telegram.me/"))?;
    let name = path.split(['/', '?', '#']).next()?;
    // Invites (+…, joinchat) and service paths are not usernames.
    let valid = name.len() >= 4
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(name, "joinchat" | "addstickers" | "addemoji" | "share" | "proxy" | "socks" | "setlanguage");
    // A link to a post (t.me/name/123) still opens the chat.
    valid.then(|| name.to_owned())
}

/// Opens the chat of @name, or the link (or t.me/name) in the browser when
/// the name does not resolve.
fn open_username(name: String, fallback: Option<String>) {
    let query = name.clone();
    run(async move { actions::resolve_username(tg(), &query).await }, move |result| match result {
        Ok(chat) => {
            with_app(|app| crate::search::remember_stranger(app, &chat));
            crate::open_chat(chat.id);
        }
        Err(err) if err.code == "USERNAME_NOT_OCCUPIED" || err.code == "USERNAME_INVALID" => {
            browse(&fallback.unwrap_or_else(|| format!("https://t.me/{name}")));
        }
        Err(err) => crate::menus::toast(&format::error(&err)),
    });
}

/// The system's browser.
fn browse(url: &str) {
    let lower = url.to_ascii_lowercase();
    // Only web and mail addresses: never a local file or program.
    if !(lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")) {
        crate::menus::toast(tr!("This link cannot be opened"));
        return;
    }
    #[cfg(windows)]
    let result: Result<(), ()> = {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::{PCWSTR, w};
        let wide: Vec<u16> = std::ffi::OsStr::new(url).encode_wide().chain(Some(0)).collect();
        let code = unsafe { ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL) };
        if code.0 as usize > 32 { Ok(()) } else { Err(()) }
    };
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn().map(drop).map_err(drop);
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn().map(drop).map_err(drop);
    if result.is_err() {
        crate::menus::toast(tr!("Could not open the browser"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(kind: &str, offset: i32, length: i32, target: Option<&str>) -> Entity {
        Entity { kind: kind.into(), offset, length, target: target.map(Into::into) }
    }

    #[test]
    fn plain_text_has_no_markdown() {
        assert_eq!(markdown("hello *world*", &[]), None);
        assert_eq!(markdown("bold", &[entity("bold", 0, 4, None)]), None);
    }

    #[test]
    fn links_and_escaping() {
        let text = "see example.com, *now*";
        let md = markdown(text, &[entity("url", 4, 11, None)]).unwrap();
        assert_eq!(md, "see [example\\.com](<https://example.com>)\\, \\*now\\*");
    }

    #[test]
    fn hidden_link_asks() {
        let md = markdown("click here", &[entity("textUrl", 6, 4, Some("https://x.org/a"))]).unwrap();
        assert!(md.ends_with("[here](<oxi-hidden:https://x.org/a>)"));
        // Text equal to the address opens directly.
        let md = markdown("x.org/a", &[entity("textUrl", 0, 7, Some("https://x.org/a"))]).unwrap();
        assert!(md.contains("(<https://x.org/a>)"));
    }

    #[test]
    fn utf16_offsets() {
        // "😀 " is 3 UTF-16 units.
        let md = markdown("😀 @name!", &[entity("mention", 3, 5, None)]).unwrap();
        assert_eq!(md, "😀 [\\@name](<oxi-mention:name>)\\!");
    }

    #[test]
    fn lines_survive() {
        let md = markdown("a\n\n  b #tag", &[entity("hashtag", 7, 4, None)]).unwrap();
        assert_eq!(md, "a\n\u{200B}\n\u{00A0}\u{00A0}b [\\#tag](<oxi-tag:#tag>)");
        let parsed = slint::StyledText::from_markdown(&md);
        assert!(parsed.is_ok(), "{parsed:?}");
    }

    #[test]
    fn markdown_parses_for_awkward_text() {
        for text in ["1. list", "# head", "> quote", "    code", "<b>html</b>", "a\n===", "`x`", "[a](b)", "- item"] {
            let full = format!("{text} t.me/x");
            let offset = full.encode_utf16().count() as i32 - 6;
            let md = markdown(&full, &[entity("url", offset, 6, None)]).unwrap();
            assert!(slint::StyledText::from_markdown(&md).is_ok(), "{text:?} -> {md:?}");
        }
    }

    #[test]
    fn telegram_links() {
        assert_eq!(telegram_username("https://t.me/durov").as_deref(), Some("durov"));
        assert_eq!(telegram_username("t.me/durov/123").as_deref(), Some("durov"));
        assert_eq!(telegram_username("tg://resolve?domain=durov").as_deref(), Some("durov"));
        assert_eq!(telegram_username("https://t.me/+AbCdEf"), None);
        assert_eq!(telegram_username("https://t.me/joinchat/abc"), None);
        assert_eq!(telegram_username("https://example.com/durov"), None);
    }
}
