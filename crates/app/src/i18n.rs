//! English-only interface text, pluralization, number formatting and bidirectional file names.

use std::sync::OnceLock;

pub use craft_i18n::Lang;
use craft_i18n::{Digits, Format, I18n, Region};

/// English source strings are used directly; no translation catalogs are embedded.
pub fn i18n() -> &'static I18n {
    static I: OnceLock<I18n> = OnceLock::new();
    I.get_or_init(|| {
        let (i, _) = I18n::new(&[], &[]);
        i.set_format(number_format("en", &Region::system()));
        i
    })
}

/// Orbit always uses English, regardless of the operating system's display language.
pub fn lang() -> Lang {
    Lang::EN
}

/// How numbers are written in the interface language `code` (#128): as the system region
/// writes them when it is in that language (Swiss German's `1’234.5`, a separator the user set in
/// Windows), else as CLDR writes the language: German `1.234,5`, French `1 234,5` (narrow
/// no-break space), Hindi `12,34,567`, Arabic `1,234.5` (CLDR writes Arabic without a country in
/// Latin digits; an Egyptian region, `ar-EG`, gives `١٬٢٣٤٫٥`).
pub fn number_format(code: &str, system: &Region) -> Format {
    let language = |tag: &str| {
        tag.split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
    };
    if language(&system.numbers) == language(code) {
        Format::new(system, Digits::Locale)
    } else {
        Format::from_tag(code)
    }
}

/// The interface language's code (`en`, `ar`…).
pub fn code() -> &'static str {
    i18n().code(lang())
}

/// Is the interface written right to left (Arabic, Persian, Hebrew, Urdu, Uyghur)? Then every
/// window is laid out from the right (winui's surfaces).
pub fn rtl() -> bool {
    i18n().rtl(lang())
}

/// Is `code` a Chinese, Japanese or Korean language? Its glyphs are denser than Latin at the
/// same size, so the interface draws its smallest text larger ([`crate::theme::font_size`]).
pub fn cjk_code(code: &str) -> bool {
    matches!(code, "zh-hans" | "zh-hant" | "ja" | "ko")
}

/// Is the interface written with Chinese, Japanese or Korean characters?
pub fn cjk() -> bool {
    cjk_code(code())
}

/// Marks `source` for the catalog (tools/i18n/extract.py) where the lookup happens later, e.g.
/// a menu label chosen here and translated when the menu is drawn. Returns it unchanged.
pub const fn mark(source: &'static str) -> &'static str {
    source
}

/// The translation of `source` (English); `source` itself when there is none.
pub fn t(source: &str) -> &str {
    i18n().t(lang(), source)
}

/// The translation of `source` in `context` (the same English with different meanings).
pub fn tc<'a>(context: &str, source: &'a str) -> &'a str {
    i18n().tc(lang(), context, source)
}

/// A count: `one` / `other` templates with `{n}` (plural rules of the language).
pub fn tn(n: u64, one: &str, other: &str) -> String {
    i18n().trn(lang(), n, one, other)
}

/// Fill a translated template's `{name}` slots.
pub fn fmt(template: &str, args: &[(&str, &str)]) -> String {
    craft_i18n::fmt(template, args)
}

/// Data that reads left to right in every language (a path, an address, a size, a date, a
/// speed): a left-to-right isolate, inside a translated sentence or alone.
pub fn ltr(value: &str) -> String {
    craft_i18n::ltr(value)
}

/// Data with a direction of its own (a file name, a description) shown alone where interface
/// text goes: a first-strong isolate, so an Arabic name reads right to left in an English window
/// and a Latin one left to right in an Arabic window (interface text is laid out in the
/// interface's direction).
pub fn isolate(value: &str) -> String {
    craft_i18n::isolate(value)
}

/// Text MDM hands to the system to draw (#919): a window's title (the taskbar, its preview and
/// tip, Alt+Tab), the notification-area icon's tip, a file dialog's title. MDM's own drawing
/// keeps a file name or a number apart from the sentence around it with Unicode's isolates
/// (FSI, LRI, RLI … PDI: [`isolate`], [`ltr`], [`fmt`]), which Unicode added in 2013 and
/// Windows 10's shell draws as boxes. They go:
///
/// - in text that reads left to right throughout (no right-to-left letter, a left-to-right
///   interface), without a trace: there is no order to keep;
/// - elsewhere each isolate becomes the older controls that keep the same order, which Windows
///   has drawn as nothing since Windows 2000 (its text boxes offer them under "Insert Unicode
///   control character"): an embedding (LRE U+202A or RLE U+202B … PDF U+202C) in the value's
///   own direction, so a Hebrew name with a Latin ".txt" reads as it does in MDM, and the
///   surrounding text's mark (LRM U+200E, RLM U+200F) on each side of it, so a number or a
///   bracket next to it stays on its own side ("71%" before an English name in an Arabic
///   title). The text outside every isolate takes the interface's direction, which the first
///   mark also tells a shell that finds a line's direction from its first letter.
///
/// What it gives has no isolate, so it gives that back as it is.
pub fn os_text(text: &str) -> String {
    os_text_in(text, rtl())
}

/// [`os_text`] for an interface written right to left (`rtl`) or not.
fn os_text_in(text: &str, rtl: bool) -> String {
    const LRI: char = '\u{2066}';
    const RLI: char = '\u{2067}';
    const FSI: char = '\u{2068}';
    const PDI: char = '\u{2069}';
    const PDF: char = '\u{202C}';
    let isolate_mark = |c: char| matches!(c, LRI | RLI | FSI | PDI);
    if !text.chars().any(isolate_mark) {
        return text.to_owned();
    }
    if !rtl && !text.chars().any(|c| strong(c) == Some(true)) {
        return text.chars().filter(|c| !isolate_mark(*c)).collect();
    }
    let mark = |rtl: bool| if rtl { '\u{200F}' } else { '\u{200E}' };
    let embedding = |rtl: bool| if rtl { '\u{202B}' } else { '\u{202A}' };
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 12);
    // Each line starts in the interface's direction, whatever its first letter.
    out.push(mark(rtl));
    // The direction of each open isolate, the paragraph's first.
    let mut open = vec![rtl];
    for (i, &c) in chars.iter().enumerate() {
        // The direction of the text this character is in.
        let here = *open.last().unwrap_or(&rtl);
        match c {
            LRI | RLI | FSI => {
                let inner = match c {
                    LRI => false,
                    RLI => true,
                    _ => first_strong(&chars[i + 1..]).unwrap_or(here),
                };
                out.push(mark(here));
                out.push(embedding(inner));
                open.push(inner);
            }
            // A PDI with no isolate open closes nothing.
            PDI if open.len() > 1 => {
                open.pop();
                out.push(PDF);
                out.push(mark(*open.last().unwrap_or(&rtl)));
            }
            PDI => {}
            '\n' => {
                out.push(c);
                out.push(mark(rtl));
            }
            _ => out.push(c),
        }
    }
    // One left open ends with the text.
    while open.len() > 1 {
        open.pop();
        out.push(PDF);
        out.push(mark(*open.last().unwrap_or(&rtl)));
    }
    out
}

/// A letter's direction: right to left (Hebrew, Arabic, Syriac, Thaana…) `Some(true)`, left to
/// right `Some(false)`; digits, spaces and punctuation have none.
fn strong(c: char) -> Option<bool> {
    use unicode_bidi::{BidiClass, bidi_class};
    match bidi_class(c) {
        BidiClass::R | BidiClass::AL => Some(true),
        BidiClass::L => Some(false),
        _ => None,
    }
}

/// The direction of a first-strong isolate's first letter, `rest` starting right after its FSI:
/// isolates inside it are skipped, its PDI ends the search.
fn first_strong(rest: &[char]) -> Option<bool> {
    let mut depth = 0usize;
    for &c in rest {
        match c {
            '\u{2066}' | '\u{2067}' | '\u{2068}' => depth += 1,
            '\u{2069}' if depth == 0 => return None,
            '\u{2069}' => depth -= 1,
            _ if depth == 0 => {
                if let Some(rtl) = strong(c) {
                    return Some(rtl);
                }
            }
            _ => {}
        }
    }
    None
}

/// `s` as a sentence of its own: its first letter a capital. A failure's reason starts in lower
/// case to follow "Error: "; alone in a paragraph it starts as a sentence does (#701). Turkish and
/// Azerbaijani write i's capital İ; a script without capitals stays as it is.
pub fn sentence_start(s: &str) -> String {
    sentence_start_in(s, code())
}

fn sentence_start_in(s: &str, language: &str) -> String {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = if first == 'i' && matches!(language, "tr" | "az") {
        "İ".to_owned()
    } else {
        first.to_uppercase().collect()
    };
    out.push_str(chars.as_str());
    out
}

/// A whole number as the interface language writes it (English: plain digits, as IDM):
/// `10200`, `10.200`, `10 200`.
pub fn integer(n: u64) -> String {
    if code() == "en" {
        n.to_string()
    } else {
        i18n().format().integer(n)
    }
}

/// `v` with `decimals` digits after the decimal mark, as the interface language writes it
/// (English: `1234.50`, as IDM; German `1.234,50`).
pub fn decimal(v: f64, decimals: usize) -> String {
    if code() == "en" {
        format!("{v:.decimals$}")
    } else {
        i18n().format().number(v, decimals)
    }
}

/// `ratio` as a percentage with `decimals` digits, as the interface language writes it (English:
/// `13.38%`, as IDM; German `13,38 %`, Turkish `%13,38`).
pub fn percent(ratio: f64, decimals: usize) -> String {
    if code() == "en" {
        format!("{:.decimals$}%", 100.0 * ratio)
    } else {
        i18n().format().percent(ratio, decimals)
    }
}

#[cfg(test)]
pub fn languages() -> Vec<(Lang, &'static str, &'static str)> {
    vec![(Lang::EN, "en", "English")]
}

#[cfg(test)]
mod tests {
    use super::*;

    const LRI: char = '\u{2066}';
    const FSI: char = '\u{2068}';
    const PDI: char = '\u{2069}';

    #[test]
    fn a_reason_alone_starts_as_a_sentence() {
        assert_eq!(
            sentence_start_in("the site sent a web page", "en"),
            "The site sent a web page"
        );
        assert_eq!(sentence_start_in("сайт отправил", "ru"), "Сайт отправил");
        assert_eq!(sentence_start_in("indirme", "tr"), "İndirme");
        assert_eq!(sentence_start_in("أرسل الموقع", "ar"), "أرسل الموقع");
        assert_eq!(sentence_start_in("", "en"), "");
    }

    /// No operating-system or saved language preference can enable a translation.
    #[test]
    fn only_english_is_available() {
        assert_eq!(code(), "en");
        assert_eq!(languages(), vec![(Lang::EN, "en", "English")]);
        assert!(i18n().from_code("sk").is_none());
        assert!(i18n().from_code("ar").is_none());
        assert_eq!(t("Options"), "Options");
    }

    #[test]
    fn data_goes_in_as_isolates() {
        // A path or an address reads left to right; a file name takes its own direction.
        assert_eq!(ltr(r"C:\dl\a.zip"), format!(r"{LRI}C:\dl\a.zip{PDI}"));
        assert_eq!(isolate("ملف.zip"), format!("{FSI}ملف.zip{PDI}"));
        // In a right-to-left sentence the values stay isolated (one isolate is kept, not
        // doubled), so a Latin name stays where the translator put it.
        let s = fmt("حذف \"{name}\"؟", &[("name", &isolate("report.zip"))]);
        assert_eq!(s, format!("حذف \"{FSI}report.zip{PDI}\"؟"));
        // English text with English values gets no marks; an Arabic name in an English
        // sentence is isolated, so it can't reorder the sentence.
        assert_eq!(
            fmt("Delete \"{name}\"?", &[("name", "a.zip")]),
            "Delete \"a.zip\"?"
        );
        assert_eq!(
            fmt("Delete \"{name}\"?", &[("name", "ملف.zip")]),
            format!("Delete \"{FSI}ملف.zip{PDI}\"?")
        );
    }

    /// The order a shell shows `text` in, without the invisible marks: the Unicode Bidi
    /// Algorithm's, the paragraph's direction `rtl`, or found from its first letter (`None`).
    fn shown(text: &str, rtl: Option<bool>) -> String {
        use unicode_bidi::{BidiInfo, Level};
        let level = rtl.map(|r| if r { Level::rtl() } else { Level::ltr() });
        let info = BidiInfo::new(text, level);
        let para = &info.paragraphs[0];
        info.reorder_line(para, para.range.clone())
            .chars()
            .filter(|c| {
                !matches!(*c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
            })
            .collect()
    }

    /// #919: what goes to the system has no isolate (Windows 10's taskbar drew them as boxes),
    /// and reads in the same order as MDM's own drawing of it.
    #[test]
    fn text_for_the_system_has_no_isolates_and_keeps_its_order() {
        let name = |n: &str| isolate(n);
        // The report's title: English, a Latin name. Only the isolates go.
        let title = format!("71% {}", name("TeraLace_1.2.3_universal.apk"));
        assert_eq!(
            os_text_in(&title, false),
            "71% TeraLace_1.2.3_universal.apk"
        );
        // No isolate: untouched, in any direction.
        for rtl in [false, true] {
            assert_eq!(
                os_text_in("Orbit Download Manager", rtl),
                "Orbit Download Manager"
            );
            assert_eq!(os_text_in("مدير التنزيل", rtl), "مدير التنزيل");
        }
        let cases: &[(String, bool)] = &[
            // A Hebrew name in an English title: it can't pull "71%" over to its side.
            (format!("71% {}", name("שלום (1).txt")), false),
            (format!("{} 71%", name("שלום")), false),
            // An English name in an Arabic title and sentence: its bracket stays its own, the
            // percentage and the Arabic words keep their places.
            (format!("71% {}", name("Report (final).zip")), true),
            (format!("{} 71%", name("Report (final)")), true),
            (
                fmt(
                    "تم تنزيل {file} إلى {folder}.",
                    &[("file", "video (2).mp4"), ("folder", &ltr(r"C:\dl\new"))],
                ),
                true,
            ),
            // Counts in an Arabic tip, one per line (the notification area's icon).
            (
                format!(
                    "Orbit Download Manager\n{}",
                    fmt(
                        "{n} تنزيلات، {speed}",
                        &[("n", &ltr("3")), ("speed", &ltr("1.2 MB/s"))]
                    )
                ),
                true,
            ),
            // An Arabic name in an Arabic title, a name with its own isolate inside.
            (format!("45% {}", name("ملف 1.zip")), true),
            (format!("x {}", name(&format!("a {} b", ltr("1 2")))), true),
        ];
        for (text, rtl) in cases {
            let os = os_text_in(text, *rtl);
            assert!(
                !os.chars().any(|c| ('\u{2066}'..='\u{2069}').contains(&c)),
                "{os:?}"
            );
            for line in text.split('\n').zip(os.split('\n')) {
                let mdm = shown(line.0, Some(*rtl));
                assert_eq!(shown(line.1, Some(*rtl)), mdm, "{text:?} as {os:?}");
                // A shell that finds the direction itself, from the first letter, too.
                if line.1.chars().any(|c| matches!(c, '\u{200E}' | '\u{200F}')) {
                    assert_eq!(shown(line.1, None), mdm, "{text:?} as {os:?}, by itself");
                }
            }
            // Given its own result, it gives it back.
            assert_eq!(os_text_in(&os, *rtl), os);
        }
        // An isolate left open, a PDI that closes nothing.
        let odd = format!("{FSI}abc {PDI}{PDI}d {LRI}e");
        for rtl in [false, true] {
            let os = os_text_in(&odd, rtl);
            assert!(!os.chars().any(|c| ('\u{2066}'..='\u{2069}').contains(&c)));
        }
        assert_eq!(os_text_in(&odd, false), "abc d e");
    }

    #[test]
    fn right_to_left_languages() {
        let i = i18n();
        for code in ["ar", "fa", "he", "ur", "ug"] {
            if let Some(l) = i.from_code(code) {
                assert!(i.rtl(l), "{code}");
            }
        }
        assert!(!i.rtl(Lang::EN));
    }

    /// #837: the languages whose glyphs the interface draws a size larger.
    #[test]
    fn cjk_languages() {
        for code in ["zh-hans", "zh-hant", "ja", "ko"] {
            assert!(cjk_code(code), "{code}");
        }
        for code in ["en", "de", "fr", "ru", "ar", "th", "hi", "vi"] {
            assert!(!cjk_code(code), "{code}");
        }
    }

    const NNBSP: char = '\u{202f}';

    /// #128: numbers are grouped and pointed as the interface language writes them, whatever the
    /// system's region (here the United States'), and as the region does when it is in the same
    /// language.
    #[test]
    fn numbers_follow_the_interface_language() {
        let us = Region::from_tag("en-US");
        for (code, n, count, size) in [
            ("en", 10_200, "10,200".to_owned(), "1,234.50".to_owned()),
            ("de", 10_200, "10.200".to_owned(), "1.234,50".to_owned()),
            (
                "fr",
                10_200,
                format!("10{NNBSP}200"),
                format!("1{NNBSP}234,50"),
            ),
            // CLDR writes Arabic without a country in Latin digits, grouped the English way.
            ("ar", 10_200, "10,200".to_owned(), "1,234.50".to_owned()),
            // Hindi groups by lakh and crore.
            (
                "hi",
                10_010_200,
                "1,00,10,200".to_owned(),
                "1,234.50".to_owned(),
            ),
        ] {
            let f = number_format(code, &us);
            assert_eq!(f.integer(n), count, "{code}");
            assert_eq!(f.number(1234.5, 2), size, "{code}");
        }
        // A region in the interface's language keeps its own way, and a separator the user set.
        let swiss = Region::from_tag("de-CH");
        assert_eq!(
            number_format("de", &swiss).integer(10_200),
            Format::from_tag("de-CH").integer(10_200)
        );
        assert_ne!(number_format("de", &swiss).integer(10_200), "10.200");
        let egypt = Region::from_tag("ar-EG");
        assert_eq!(number_format("ar", &egypt).integer(10_200), "١٠٬٢٠٠");
        assert_eq!(number_format("de", &egypt).integer(10_200), "10.200");
        let mut own = Region::from_tag("de-DE");
        own.grouping = Some(Some('\''));
        assert_eq!(number_format("de", &own).integer(10_200), "10'200");
        // Catalog codes with a country or a script are read too.
        assert_eq!(number_format("pt-br", &us).integer(10_200), "10.200");
        assert_eq!(number_format("zh-hans", &us).integer(10_200), "10,200");
    }

    /// Counts, sizes and speeds take their numbers from that format, beside the catalogs' units
    /// (the way the list, the progress window and the playlist dialog write them).
    #[test]
    fn english_counts_sizes_and_speeds() {
        let i = i18n();
        assert!(
            i.trn(Lang::EN, 2, "{n} video", "{n} videos")
                .contains("2 videos")
        );
        assert!(i.measure(Lang::EN, 1.5, 2, "{n} MB").contains("1.50"));
    }
}
