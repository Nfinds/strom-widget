//! Tiny runtime language switch (German / English) without external crates.

use std::sync::atomic::{AtomicBool, Ordering};

static EN: AtomicBool = AtomicBool::new(false);
static USD: AtomicBool = AtomicBool::new(false);

pub fn set_en(en: bool) {
    EN.store(en, Ordering::Relaxed);
}

pub fn is_en() -> bool {
    EN.load(Ordering::Relaxed)
}

/// Pick the German or the English text for a fixed string.
pub fn t(de: &'static str, en: &'static str) -> &'static str {
    if is_en() {
        en
    } else {
        de
    }
}

/// Decimal separator for the current language.
pub fn dec() -> &'static str {
    if is_en() {
        "."
    } else {
        ","
    }
}

/// Like `format!`, with a German and an English template (same arguments, same order).
macro_rules! tr {
    ($de:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_en() {
            format!($en $(, $arg)*)
        } else {
            format!($de $(, $arg)*)
        }
    };
}

/// Default language: German if the Windows display language is German, otherwise English.
pub fn system_default_lang() -> &'static str {
    use windows::Win32::Globalization::GetUserDefaultUILanguage;
    // Primary language id 0x07 = German (de-DE, de-AT, de-CH, ...).
    let langid = unsafe { GetUserDefaultUILanguage() };
    if langid & 0x3ff == 0x07 {
        "de"
    } else {
        "en"
    }
}

pub fn set_usd(usd: bool) {
    USD.store(usd, Ordering::Relaxed);
}

pub fn is_usd() -> bool {
    USD.load(Ordering::Relaxed)
}

/// Cent unit of the current currency: "ct" (euro) or "\u{00a2}" (dollar).
pub fn cent_unit() -> &'static str {
    if is_usd() {
        "\u{00a2}"
    } else {
        "ct"
    }
}

/// A money amount given in cents, e.g. "1,23 \u{20ac}" or "$1.23".
pub fn money(ct: f64) -> String {
    let v = format!("{:.2}", ct / 100.0).replace('.', dec());
    if is_usd() {
        format!("${}", v)
    } else {
        format!("{} \u{20ac}", v)
    }
}

/// Default currency: US dollar if the Windows region is the United States, otherwise euro.
pub fn system_default_currency() -> &'static str {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buf = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n > 0 {
        let name = String::from_utf16_lossy(&buf[..(n as usize).saturating_sub(1)]).to_uppercase();
        if name.ends_with("-US") {
            return "USD";
        }
    }
    "EUR"
}
