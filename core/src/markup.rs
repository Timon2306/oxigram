//! Telegram Desktop's typing shortcuts for formatting: `**bold**`,
//! `__italic__`, `~~strike~~`, `||spoiler||`, `` `code` `` and
//! ```` ```pre``` ````. The markers are removed and become entities, whose
//! offsets count UTF-16 units as Telegram expects.

use grammers_client::tl;

/// Markers that wrap a span, longest first so ``` wins over `.
const MARKERS: [(&str, Kind); 6] = [
    ("```", Kind::Pre),
    ("**", Kind::Bold),
    ("__", Kind::Italic),
    ("~~", Kind::Strike),
    ("||", Kind::Spoiler),
    ("`", Kind::Code),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Bold,
    Italic,
    Strike,
    Spoiler,
    Code,
    Pre,
}

/// Splits `text` into the plain text to send and its formatting.
pub fn parse(text: &str) -> (String, Vec<tl::enums::MessageEntity>) {
    let mut out = String::with_capacity(text.len());
    let mut units: i32 = 0; // UTF-16 length of `out`
    let mut open: Vec<(Kind, i32)> = Vec::new();
    let mut entities = Vec::new();
    let mut rest = text;

    while !rest.is_empty() {
        // Inside code nothing is parsed until the same marker closes it.
        if let Some(&(kind, start)) = open.last() {
            if matches!(kind, Kind::Code | Kind::Pre) {
                let marker = if kind == Kind::Pre { "```" } else { "`" };
                if let Some(end) = rest.find(marker) {
                    let body = &rest[..end];
                    // A language tag on the first line of a block is dropped.
                    let body = if kind == Kind::Pre && units == start {
                        strip_language(body)
                    } else {
                        body
                    };
                    out.push_str(body);
                    units += utf16_len(body);
                    open.pop();
                    if units > start {
                        entities.push(entity(kind, start, units - start));
                    }
                    rest = &rest[end + marker.len()..];
                    continue;
                }
                // Never closed: the marker was literal after all.
                open.pop();
                let marker_units = utf16_len(marker);
                out.insert_str(byte_index(&out, start), marker);
                units += marker_units;
                for entity in &mut entities {
                    shift_after(entity, start, marker_units);
                }
                continue;
            }
        }

        if let Some(&(marker, kind)) = MARKERS.iter().find(|(m, _)| rest.starts_with(m)) {
            if let Some(index) = open.iter().rposition(|(k, _)| *k == kind) {
                // Closing. Markers opened inside it and never closed were
                // literal text: put them back where they were typed.
                let inner: Vec<(Kind, i32)> = open.drain(index + 1..).collect();
                let (_, start) = open.pop().expect("index is in range");
                for (inner_kind, at) in inner.into_iter().rev() {
                    let text = MARKERS.iter().find(|(_, k)| *k == inner_kind).expect("known kind").0;
                    out.insert_str(byte_index(&out, at), text);
                    units += utf16_len(text);
                    for entity in &mut entities {
                        shift_after(entity, at, utf16_len(text));
                    }
                }
                if units > start {
                    entities.push(entity(kind, start, units - start));
                }
            } else {
                open.push((kind, units));
            }
            rest = &rest[marker.len()..];
            continue;
        }

        let c = rest.chars().next().expect("rest is not empty");
        out.push(c);
        units += c.len_utf16() as i32;
        rest = &rest[c.len_utf8()..];
    }

    // Markers never closed stay as typed: put them back, last first.
    while let Some((kind, start)) = open.pop() {
        let marker = MARKERS.iter().find(|(_, k)| *k == kind).expect("known kind").0;
        let marker_units = utf16_len(marker);
        out.insert_str(byte_index(&out, start), marker);
        for entity in &mut entities {
            shift_after(entity, start, marker_units);
        }
    }
    entities.sort_by_key(offset_of);
    (out, entities)
}

fn strip_language(body: &str) -> &str {
    match body.split_once('\n') {
        Some((first, rest)) if !first.is_empty() && !first.contains(' ') => rest,
        _ => body,
    }
}

fn utf16_len(text: &str) -> i32 {
    text.encode_utf16().count() as i32
}

/// The byte index in `text` of the given UTF-16 offset.
fn byte_index(text: &str, units: i32) -> usize {
    let mut count = 0;
    for (index, c) in text.char_indices() {
        if count >= units {
            return index;
        }
        count += c.len_utf16() as i32;
    }
    text.len()
}

fn entity(kind: Kind, offset: i32, length: i32) -> tl::enums::MessageEntity {
    use tl::types as t;
    match kind {
        Kind::Bold => t::MessageEntityBold { offset, length }.into(),
        Kind::Italic => t::MessageEntityItalic { offset, length }.into(),
        Kind::Strike => t::MessageEntityStrike { offset, length }.into(),
        Kind::Spoiler => t::MessageEntitySpoiler { offset, length }.into(),
        Kind::Code => t::MessageEntityCode { offset, length }.into(),
        Kind::Pre => t::MessageEntityPre {
            offset,
            length,
            language: String::new(),
        }
        .into(),
    }
}

fn offset_of(entity: &tl::enums::MessageEntity) -> i32 {
    use tl::enums::MessageEntity as E;
    match entity {
        E::Bold(e) => e.offset,
        E::Italic(e) => e.offset,
        E::Strike(e) => e.offset,
        E::Spoiler(e) => e.offset,
        E::Code(e) => e.offset,
        E::Pre(e) => e.offset,
        _ => 0,
    }
}

/// Moves an entity right when text was inserted at or before its start.
fn shift_after(entity: &mut tl::enums::MessageEntity, at: i32, by: i32) {
    use tl::enums::MessageEntity as E;
    let offset = match entity {
        E::Bold(e) => &mut e.offset,
        E::Italic(e) => &mut e.offset,
        E::Strike(e) => &mut e.offset,
        E::Spoiler(e) => &mut e.offset,
        E::Code(e) => &mut e.offset,
        E::Pre(e) => &mut e.offset,
        _ => return,
    };
    if *offset >= at {
        *offset += by;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(text: &str) -> (String, Vec<(&'static str, i32, i32)>) {
        use tl::enums::MessageEntity as E;
        let (plain, entities) = parse(text);
        let list = entities
            .iter()
            .map(|entity| match entity {
                E::Bold(e) => ("bold", e.offset, e.length),
                E::Italic(e) => ("italic", e.offset, e.length),
                E::Strike(e) => ("strike", e.offset, e.length),
                E::Spoiler(e) => ("spoiler", e.offset, e.length),
                E::Code(e) => ("code", e.offset, e.length),
                E::Pre(e) => ("pre", e.offset, e.length),
                _ => ("other", 0, 0),
            })
            .collect();
        (plain, list)
    }

    #[test]
    fn plain_text_is_untouched() {
        assert_eq!(spans("привет, мир"), ("привет, мир".into(), vec![]));
    }

    #[test]
    fn markers_become_entities() {
        assert_eq!(
            spans("a **b** __c__ ~~d~~ ||e|| `f`"),
            (
                "a b c d e f".into(),
                vec![("bold", 2, 1), ("italic", 4, 1), ("strike", 6, 1), ("spoiler", 8, 1), ("code", 10, 1)]
            )
        );
    }

    #[test]
    fn offsets_count_utf16_units() {
        // The emoji is two UTF-16 units.
        assert_eq!(spans("😀 **жир**"), ("😀 жир".into(), vec![("bold", 3, 3)]));
    }

    #[test]
    fn code_keeps_markers_inside() {
        assert_eq!(spans("`**x**`"), ("**x**".into(), vec![("code", 0, 5)]));
        assert_eq!(
            spans("```rust\nfn main() {}\n```"),
            ("fn main() {}\n".into(), vec![("pre", 0, 13)])
        );
    }

    #[test]
    fn unclosed_markers_stay_literal() {
        assert_eq!(spans("2 ** 3"), ("2 ** 3".into(), vec![]));
        assert_eq!(spans("a `b"), ("a `b".into(), vec![]));
        assert_eq!(spans("**a __b**"), ("a __b".into(), vec![("bold", 0, 5)]));
    }

    #[test]
    fn nesting_works() {
        assert_eq!(
            spans("**жирный __и курсив__**"),
            ("жирный и курсив".into(), vec![("bold", 0, 15), ("italic", 7, 8)])
        );
    }
}
