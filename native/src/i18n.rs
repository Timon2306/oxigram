//! The interface language: English is the source, translations live in
//! `native/translations/<lang>/LC_MESSAGES/oxigram-native.po`. The same
//! files serve both sides: Slint bundles them for `@tr(…)` in app.slint, and
//! this module reads them for texts made in Rust (`tr!`, `trn!`).
//!
//! The language is the one chosen in the settings, or the system's (ru*,
//! uk*, en*; anything else is English). Switching applies at once.

use std::collections::HashMap;
use std::sync::RwLock;

/// (code, name in its own language), in the settings' order.
pub const LANGUAGES: [(&str, &str); 3] = [("en", "English"), ("ru", "Русский"), ("uk", "Українська")];

const RU: &str = include_str!("../translations/ru/LC_MESSAGES/oxigram-native.po");
const UK: &str = include_str!("../translations/uk/LC_MESSAGES/oxigram-native.po");

/// A parsed catalog: msgid → forms (one, or one per plural form).
#[derive(Default)]
pub struct Catalog {
    entries: HashMap<String, Vec<String>>,
}

struct State {
    code: &'static str,
    catalog: Catalog,
}

static STATE: RwLock<Option<State>> = RwLock::new(None);

/// Unescapes the inside of a .po string.
fn unquote(line: &str) -> String {
    let inner = line.trim();
    let inner = inner.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or("");
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// Reads a .po file (msgid, msgid_plural, msgstr, msgstr[N], continued
/// lines; comments and msgctxt are skipped).
pub fn parse(text: &str) -> Catalog {
    #[derive(PartialEq, Clone, Copy)]
    enum Field {
        None,
        Id,
        Plural,
        Str(usize),
    }
    let mut catalog = Catalog::default();
    let mut id = String::new();
    let mut forms: Vec<String> = Vec::new();
    let mut field = Field::None;
    let mut flush = |id: &mut String, forms: &mut Vec<String>| {
        if !id.is_empty() && forms.iter().any(|f| !f.is_empty()) {
            catalog.entries.insert(std::mem::take(id), std::mem::take(forms));
        }
        id.clear();
        forms.clear();
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("msgid_plural ") {
            field = Field::Plural;
            let _ = unquote(rest);
        } else if let Some(rest) = line.strip_prefix("msgid ") {
            flush(&mut id, &mut forms);
            id = unquote(rest);
            field = Field::Id;
        } else if let Some(rest) = line.strip_prefix("msgstr[") {
            let (index, value) = rest.split_once(']').unwrap_or(("0", rest));
            let index: usize = index.parse().unwrap_or(0);
            if forms.len() <= index {
                forms.resize(index + 1, String::new());
            }
            forms[index] = unquote(value);
            field = Field::Str(index);
        } else if let Some(rest) = line.strip_prefix("msgstr ") {
            forms = vec![unquote(rest)];
            field = Field::Str(0);
        } else if line.starts_with("msgctxt") {
            field = Field::None;
        } else if line.starts_with('"') {
            let more = unquote(line);
            match field {
                Field::Id => id.push_str(&more),
                Field::Str(i) => {
                    if let Some(form) = forms.get_mut(i) {
                        form.push_str(&more);
                    }
                }
                Field::Plural | Field::None => {}
            }
        }
    }
    flush(&mut id, &mut forms);
    catalog
}

impl Catalog {
    pub fn get(&self, id: &str) -> Option<&Vec<String>> {
        self.entries.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }
}

/// Russian and Ukrainian plural forms: 1, 2-4, 5+ (21 is "one", 11 is "many").
fn slavic(n: i64) -> usize {
    let n = n.unsigned_abs();
    if n % 10 == 1 && n % 100 != 11 {
        0
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        1
    } else {
        2
    }
}

/// The plural form index of `n` in `code`.
pub fn plural_index(code: &str, n: i64) -> usize {
    match code {
        "ru" | "uk" => slavic(n),
        _ => usize::from(n != 1),
    }
}

/// The language for the setting (`""` = the system's).
pub fn resolve(setting: &str) -> &'static str {
    let wanted = if setting.is_empty() {
        sys_locale::get_locale().unwrap_or_default().to_lowercase()
    } else {
        setting.to_lowercase()
    };
    if wanted.starts_with("ru") {
        "ru"
    } else if wanted.starts_with("uk") || wanted.starts_with("ua") {
        "uk"
    } else {
        "en"
    }
}

/// Switches the language of the Rust texts and of the window.
pub fn set_language(setting: &str) {
    let code = resolve(setting);
    let catalog = match code {
        "ru" => parse(RU),
        "uk" => parse(UK),
        _ => Catalog::default(),
    };
    if let Ok(mut state) = STATE.write() {
        *state = Some(State { code, catalog });
    }
    // English is the source: "" selects the untranslated texts.
    let _ = slint::select_bundled_translation(if code == "en" { "" } else { code });
}

/// The current language's code.
pub fn current() -> &'static str {
    STATE.read().ok().and_then(|s| s.as_ref().map(|s| s.code)).unwrap_or("en")
}

pub fn tr(text: &str) -> String {
    let state = STATE.read();
    let found = state
        .as_ref()
        .ok()
        .and_then(|s| s.as_ref())
        .and_then(|s| s.catalog.get(text))
        .and_then(|forms| forms.first())
        .filter(|t| !t.is_empty())
        .cloned();
    found.unwrap_or_else(|| text.to_owned())
}

pub fn trn(singular: &str, plural: &str, n: i64) -> String {
    let state = STATE.read();
    let state = state.as_ref().ok().and_then(|s| s.as_ref());
    let form = state.and_then(|s| {
        let forms = s.catalog.get(singular)?;
        forms.get(plural_index(s.code, n)).filter(|t| !t.is_empty()).cloned()
    });
    let text = form.unwrap_or_else(|| if n == 1 { singular } else { plural }.to_owned());
    text.replace("{n}", &crate::format::number(n))
}

/// Installs the translator for the backend's own texts.
pub fn install() {
    oxigram_core::i18n::install(tr, trn);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plural_forms() {
        let forms: Vec<usize> = [1, 2, 4, 5, 11, 12, 14, 21, 22, 25, 101, 111].iter().map(|n| slavic(*n)).collect();
        assert_eq!(forms, vec![0, 1, 1, 2, 2, 2, 2, 0, 1, 2, 0, 2]);
        assert_eq!(plural_index("en", 1), 0);
        assert_eq!(plural_index("en", 2), 1);
    }

    #[test]
    fn parses_po() {
        let po = r#"
msgid ""
msgstr ""
"Plural-Forms: nplurals=3;\n"

# comment
msgid "Hello"
msgstr "Привет"

msgid "{n} member"
msgid_plural "{n} members"
msgstr[0] "{n} участник"
msgstr[1] "{n} участника"
msgstr[2] "{n} участников"

msgid "Long "
"text"
msgstr "Длинный "
"текст \"в кавычках\""
"#;
        let c = parse(po);
        assert_eq!(c.get("Hello").unwrap()[0], "Привет");
        assert_eq!(c.get("{n} member").unwrap()[2], "{n} участников");
        assert_eq!(c.get("Long text").unwrap()[0], "Длинный текст \"в кавычках\"");
        assert!(c.get("").is_none());
    }

    /// Reads the string literal starting at `text` (just after its opening
    /// quote): its unescaped value.
    fn literal(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(other) => out.push(other),
                    None => break,
                },
                c => out.push(c),
            }
        }
        out
    }

    /// Every (msgid, is_plural) marked for translation in the sources:
    /// `@tr("…")` in .slint, `tr!("…")` and `trn!("…", …)` in Rust.
    fn marked() -> Vec<(String, bool, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = vec![root.join("ui").join("app.slint")];
        for dir in [root.join("src"), root.join("../core/src"), root.join("../core/src/store")] {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("i18n.rs") {
                    files.push(path);
                }
            }
        }
        let mut out = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            for (marker, plural_marker) in [("@tr(\"", false), ("tr!(\"", false), ("trn!(\"", true)] {
                for (at, _) in text.match_indices(marker) {
                    // "trn!(" also contains "tr!(": skip it as a plain one.
                    if marker == "tr!(\"" && text[..at].ends_with('t') {
                        continue;
                    }
                    let rest = &text[at + marker.len()..];
                    let id = literal(rest);
                    // @tr("one" | "many" % n) is a plural in Slint.
                    let after = &rest[id.len().min(rest.len())..];
                    let plural = plural_marker || (marker == "@tr(\"" && after.trim_start_matches('"').trim_start().starts_with('|'));
                    out.push((id, plural, name.clone()));
                }
            }
        }
        out
    }

    #[test]
    fn every_text_is_translated() {
        let mut missing = Vec::new();
        for (code, po) in [("ru", RU), ("uk", UK)] {
            let catalog = parse(po);
            for (id, plural, file) in marked() {
                match catalog.get(&id) {
                    Some(forms) if !plural && forms.first().is_some_and(|f| !f.is_empty()) => {}
                    Some(forms) if plural && forms.len() == 3 && forms.iter().all(|f| !f.is_empty()) => {}
                    _ => missing.push(format!("{code}: {id:?} ({file})")),
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "{} untranslated:\n{}", missing.len(), missing.join("\n"));
    }

    #[test]
    fn english_is_the_fallback() {
        set_language("en");
        assert_eq!(tr("Language"), "Language");
        assert_eq!(trn("{n} member", "{n} members", 2), "2 members");
        assert_eq!(tr("No such text anywhere"), "No such text anywhere");
    }

    #[test]
    fn languages_resolve() {
        assert_eq!(resolve("ru-RU"), "ru");
        assert_eq!(resolve("uk"), "uk");
        assert_eq!(resolve("de-DE"), "en");
        assert_eq!(resolve("en"), "en");
    }
}
