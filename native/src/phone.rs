//! The phone step of the login: the country (flag and code) beside the
//! number, its mask as the placeholder, grouping while typing, and a full
//! "+…" number typed or pasted switching the country by itself.

use std::cell::{Cell, RefCell};

use slint::{ModelRc, SharedString, VecModel};

use crate::countries::{self, Country};
use crate::{AppWindow, CountryItem, ui};

thread_local! {
    static CURRENT: Cell<Option<&'static Country>> = const { Cell::new(None) };
    /// The countries in the open list, in its order.
    static SHOWN: RefCell<Vec<&'static Country>> = const { RefCell::new(Vec::new()) };
    static SERIAL: Cell<i32> = const { Cell::new(0) };
}

fn name(country: &'static Country) -> String {
    crate::i18n::tr(country.name)
}

pub fn wire(window: &AppWindow) {
    // The system's country to start with (ru-RU → Russia), or none.
    let start = sys_locale::get_locale().and_then(|l| countries::from_locale(&l));
    set_country(window, start);
    window.on_phone_edited(edited);
    window.on_country_filter(|query| {
        let found = countries::search(&query, name);
        let items: Vec<CountryItem> = found
            .iter()
            .map(|c| CountryItem {
                flag: countries::flag(c.iso).into(),
                name: name(c).into(),
                dial: format!("+{}", c.dial).into(),
            })
            .collect();
        SHOWN.with(|s| *s.borrow_mut() = found);
        if let Some(ui) = ui() {
            ui.set_country_items(ModelRc::new(VecModel::from(items)));
        }
    });
    window.on_country_pick(|index| {
        let picked = SHOWN.with(|s| s.borrow().get(index.max(0) as usize).copied());
        let Some(ui) = ui() else { return };
        if let Some(country) = picked {
            set_country(&ui, Some(country));
            // The number typed so far takes the new country's grouping.
            let digits = countries::digits(&ui.get_phone_text());
            show_number(&ui, &countries::format(country, &digits));
        }
        ui.set_country_open(false);
    });
}

/// After a language change: the country's name in the new language.
pub fn relabel(ui: &AppWindow) {
    set_country(ui, CURRENT.with(Cell::get));
}

/// Shows a country (flag, code, mask, name) — after a language change too.
pub fn set_country(ui: &AppWindow, country: Option<&'static Country>) {
    CURRENT.with(|c| c.set(country));
    match country {
        Some(c) => {
            ui.set_phone_flag(countries::flag(c.iso).into());
            ui.set_phone_dial(format!("+{}", c.dial).into());
            ui.set_phone_mask(c.mask.into());
            ui.set_phone_country(name(c).into());
        }
        None => {
            ui.set_phone_flag(SharedString::new());
            ui.set_phone_dial(SharedString::new());
            ui.set_phone_mask(SharedString::new());
            ui.set_phone_country(SharedString::new());
        }
    }
}

/// Puts `text` into the number field, the cursor after it.
fn show_number(ui: &AppWindow, text: &str) {
    ui.set_phone_text(text.into());
    ui.set_phone_text_bytes(text.len() as i32);
    let serial = SERIAL.with(|s| {
        s.set(s.get().wrapping_add(1));
        s.get()
    });
    ui.set_phone_text_serial(serial);
}

/// Each change of the number field.
fn edited(text: SharedString) {
    let Some(ui) = ui() else { return };
    let text = text.to_string();
    // A whole number with "+": its code picks the country.
    if text.trim_start().starts_with('+') {
        if let Some((country, rest)) = countries::split(&text) {
            set_country(&ui, Some(country));
            show_number(&ui, &countries::format(country, &rest));
        }
        return;
    }
    let Some(country) = CURRENT.with(Cell::get) else { return };
    let formatted = countries::format(country, &countries::digits(&text));
    // Only when grouping changed something, so the cursor is left alone otherwise.
    if formatted != text {
        show_number(&ui, &formatted);
    }
}

/// The number for Telegram: "+<code><digits>", or the field as typed when
/// it already starts with "+" or no country is chosen.
pub fn full_number(field: &str) -> String {
    let field = field.trim();
    if field.starts_with('+') {
        return field.to_owned();
    }
    match CURRENT.with(Cell::get) {
        Some(country) => format!("+{}{}", country.dial, countries::digits(field)),
        None => field.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_numbers() {
        CURRENT.with(|c| c.set(countries::by_iso("UA")));
        assert_eq!(full_number("67 123 45 67"), "+380671234567");
        assert_eq!(full_number("+7 916 000 00 00"), "+7 916 000 00 00");
        CURRENT.with(|c| c.set(None));
        assert_eq!(full_number("123"), "123");
    }
}
