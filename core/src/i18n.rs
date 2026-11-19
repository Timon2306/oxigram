//! Translation hook for the few texts the backend writes itself (service
//! messages, notifications). The app installs its translator at start-up;
//! without one the English source text is used.

use std::sync::OnceLock;

type Translate = fn(&str) -> String;
type TranslatePlural = fn(&str, &str, i64) -> String;

static TR: OnceLock<Translate> = OnceLock::new();
static TRN: OnceLock<TranslatePlural> = OnceLock::new();

/// Installs the app's translator (once).
pub fn install(tr: Translate, trn: TranslatePlural) {
    let _ = TR.set(tr);
    let _ = TRN.set(trn);
}

/// `text` in the user's language.
pub fn tr(text: &str) -> String {
    match TR.get() {
        Some(tr) => tr(text),
        None => text.to_owned(),
    }
}

/// The form of `singular` / `plural` for `n`, in the user's language, with
/// `{n}` replaced by the number.
pub fn trn(singular: &str, plural: &str, n: i64) -> String {
    match TRN.get() {
        Some(trn) => trn(singular, plural, n),
        None => if n == 1 { singular } else { plural }.replace("{n}", &n.to_string()),
    }
}

/// `tr!("Text")`, or `tr!("{} joined", name)`: each `{}` takes the next
/// argument (after translation, so translators may move it).
#[macro_export]
macro_rules! tr {
    ($text:literal) => {
        $crate::i18n::tr($text)
    };
    ($text:literal, $($arg:expr),+ $(,)?) => {{
        let mut out = $crate::i18n::tr($text);
        $( out = out.replacen("{}", &$arg.to_string(), 1); )+
        out
    }};
}

/// `trn!("{n} member", "{n} members", n)`.
#[macro_export]
macro_rules! trn {
    ($one:literal, $many:literal, $n:expr) => {
        $crate::i18n::trn($one, $many, ($n) as i64)
    };
}
