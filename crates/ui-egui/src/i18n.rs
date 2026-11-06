// Interface-language lookup. English UI wording is sourced from i18n/en.tsv; stable IDs are
// resolved by the generated catalog map and locale parsing/matching stays in craft-i18n.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

use craft_i18n::{I18n, Lang};

include!(concat!(env!("OUT_DIR"), "/i18n_source_ids.rs"));

/// Follow the platform's preferred language until the user chooses one explicitly.
pub const AUTO: &str = "auto";

/// Languages shown by NoteCraft. XA/XB are ordinary catalog-backed locales whose translations
/// mark the English source; a missing row deliberately falls back to unmarked English.
static EMBEDDED: &craft_i18n::Embedded = &[
    ("en-XA", "", include_str!("../../../i18n/en-XA.tsv")),
    ("ar-XB", "", include_str!("../../../i18n/ar-XB.tsv")),
    ("ar", "", include_str!("../../../i18n/ar.tsv")),
    ("de", "", include_str!("../../../i18n/de.tsv")),
    ("es", "", include_str!("../../../i18n/es.tsv")),
    ("fa", "", include_str!("../../../i18n/fa.tsv")),
    ("fr", "", include_str!("../../../i18n/fr.tsv")),
    ("he", "", include_str!("../../../i18n/he.tsv")),
    ("hi", "", include_str!("../../../i18n/hi.tsv")),
    ("ja", "", include_str!("../../../i18n/ja.tsv")),
    ("ko", "", include_str!("../../../i18n/ko.tsv")),
    ("ru", "", include_str!("../../../i18n/ru.tsv")),
    ("ur", "", include_str!("../../../i18n/ur.tsv")),
    ("zh", "", include_str!("../../../i18n/zh.tsv")),
];
static OFFERED: &[&str] = &["en-XA", "ar-XB", "ar", "de", "es", "fa", "fr", "he", "hi", "ja", "ko", "ru", "ur", "zh"];
static PSEUDO_LOOKED_UP: LazyLock<Mutex<std::collections::BTreeSet<String>>> = LazyLock::new(|| Mutex::new(std::collections::BTreeSet::new()));
static AR_XB_RENDERED: LazyLock<Mutex<BTreeMap<String, &'static str>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));

static I18N: LazyLock<I18n> = LazyLock::new(|| {
    let (i18n, problems) = I18n::new(EMBEDDED, OFFERED);
    problems.iter().for_each(|problem| log::warn!("interface catalogs: {problem}"));
    i18n
});

thread_local! {
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::EN) };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageChoice {
    pub code: String,
    pub name: String,
}

/// Languages NoteCraft can actually display, with their names in their own language.
pub fn available_languages() -> Vec<LanguageChoice> {
    let mut languages: Vec<LanguageChoice> = I18N
        .languages()
        .into_iter()
        .map(|language| LanguageChoice { code: canonical_code(I18N.code(language)), name: I18N.name(language).to_string() })
        .collect();
    languages.sort_by_key(|language| match language.code.as_str() {
        "en" => 0,
        "en-XA" => 2,
        "ar-XB" => 3,
        _ => 1,
    });
    languages
}

/// Code for the locale currently drawing this UI, resolving `auto` to its selected system locale.
pub fn current_code() -> String {
    canonical_code(I18N.code(CURRENT.get()))
}

fn canonical_code(code: &str) -> String {
    match code {
        "en-xa" => "en-XA".to_string(),
        "ar-xb" => "ar-XB".to_string(),
        _ => code.to_string(),
    }
}

/// Canonicalize a saved language code. Unknown or obsolete values return to automatic mode.
pub fn normalize_preference(preference: &str) -> String {
    if preference.eq_ignore_ascii_case(AUTO) {
        return AUTO.to_string();
    }
    if preference.eq_ignore_ascii_case("en-XA") {
        return "en-XA".to_string();
    }
    if preference.eq_ignore_ascii_case("ar-XB") {
        return "ar-XB".to_string();
    }
    I18N.from_code(preference).map_or_else(|| AUTO.to_string(), |language| I18N.code(language).to_string())
}

/// Resolve a preference against ordered system locale tags. This pure form keeps locale choice
/// deterministic in tests; the running app passes the platform's preferred tags.
pub fn resolve_preference(preference: &str, system_tags: &[&str]) -> String {
    let preference = normalize_preference(preference);
    if preference != AUTO {
        return preference;
    }
    let language =
        I18N.match_tags(system_tags.iter().copied().filter(|tag| !tag.eq_ignore_ascii_case("en-XA") && !tag.eq_ignore_ascii_case("ar-XB")));
    I18N.code(language).to_string()
}

/// Apply a saved preference for this UI thread. Automatic mode uses the shared runtime's
/// platform-aware system-language detection and falls back to English when no catalog matches.
pub fn set_current(preference: &str) {
    let preference = normalize_preference(preference);
    let language = match preference.as_str() {
        AUTO => I18N.system_lang(),
        _ => I18N.from_code(&preference).unwrap_or(Lang::EN),
    };
    CURRENT.set(language);
}

/// Translate one source string in the current language. Missing rows, including pseudo-catalog
/// rows, remain English so the pseudo locale can expose catalog coverage gaps.
pub fn t(source: &str) -> &str {
    let Some(id) = source_id(source) else {
        return source;
    };
    t_id(id)
}

/// Translate a stable UI catalog ID, falling back to the English value in `i18n/en.tsv`.
pub fn t_id(id: &str) -> &'static str {
    let language = CURRENT.get();
    if is_pseudo(language) {
        PSEUDO_LOOKED_UP.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(id.to_owned());
    }
    lookup_id(language, id)
}

fn lookup_id(language: Lang, id: &str) -> &'static str {
    let Some(english) = english_text(id) else {
        log::warn!("interface catalog has no English value for ID {id:?}");
        return "[missing UI translation]";
    };
    let translated = I18N.tid(language, id, english);
    if I18N.code(language) != "ar-xb" || translated == english {
        return translated;
    }
    let mut rendered = AR_XB_RENDERED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    rendered.entry(translated.to_owned()).or_insert_with(move || Box::leak(pseudo_render(language, translated).into_boxed_str()))
}

fn pseudo_render(language: Lang, text: &str) -> String {
    if I18N.code(language) == "ar-xb" { text.replace("[[RLI]]", "\u{2067}").replace("[[PDI]]", "\u{2069}") } else { text.to_owned() }
}

/// Source strings observed by the catalog runtime, for UI coverage audits.
pub fn pseudo_looked_up() -> std::collections::BTreeSet<String> {
    PSEUDO_LOOKED_UP.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
}

fn is_pseudo(language: Lang) -> bool {
    matches!(I18N.code(language), "en-xa" | "ar-xb")
}

/// Remove all ASCII pseudo delimiters while preserving source and dynamic wording.
pub fn pseudo_unmarked(text: &str) -> String {
    text.replace("$$$", "").replace("@@@", "").replace("\u{2067}", "").replace("\u{2069}", "").replace("~!~", "")
}

/// Keep runtime-provided text unchanged. In pseudo locales, only a catalog lookup may mark it;
/// wrapping arbitrary dynamic text here would conceal missing extraction.
pub fn pseudo_dynamic(text: &str) -> String {
    text.to_string()
}

/// Translate and format a user-facing message with named placeholders.
pub fn format_message(source: &str, args: &[(&str, &str)]) -> String {
    match source_id(source) {
        Some(id) => format_message_id(id, args),
        None => craft_i18n::fmt(source, args),
    }
}

/// Translate and format a message by stable catalog ID.
pub fn format_message_id(id: &str, args: &[(&str, &str)]) -> String {
    let language = CURRENT.get();
    if is_pseudo(language) {
        PSEUDO_LOOKED_UP.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(id.to_owned());
    }
    pseudo_render(language, &craft_i18n::fmt(t_id(id), args))
}

/// Format a count with the locale's plural rule, translating both the singular and plural forms.
pub fn format_count(one: &str, other: &str, count: u64) -> String {
    let language = CURRENT.get();
    let pair = format!("{one}|{other}");
    if let Some(id) = source_id(&pair) {
        return format_count_id(id, count);
    }
    I18N.trn(language, count, one, other)
}

/// Format a plural message by stable catalog ID. The English catalog stores its singular and
/// plural source forms separated by `|`; craft-i18n selects the locale's plural form.
pub fn format_count_id(id: &str, count: u64) -> String {
    let Some(pair) = english_text(id) else {
        log::warn!("interface catalog has no English plural value for ID {id:?}");
        return count.to_string();
    };
    let Some((one, other)) = pair.split_once('|') else {
        log::warn!("interface catalog plural ID {id:?} must contain one|other forms");
        return pair.to_string();
    };
    let language = CURRENT.get();
    if is_pseudo(language) {
        PSEUDO_LOOKED_UP.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(id.to_owned());
    }
    pseudo_render(language, &I18N.trn(language, count, one, other))
}

/// Format a plural template that also contains named placeholders after its count is selected.
pub fn format_count_message_id(id: &str, count: u64, args: &[(&str, &str)]) -> String {
    let Some(pair) = english_text(id) else {
        log::warn!("interface catalog has no English plural value for ID {id:?}");
        return id.to_string();
    };
    let Some((one, other)) = pair.split_once('|') else {
        log::warn!("interface catalog plural ID {id:?} must contain one|other forms");
        return pair.to_string();
    };
    let language = CURRENT.get();
    if is_pseudo(language) {
        PSEUDO_LOOKED_UP.lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert(id.to_owned());
    }
    let translated = I18N.trn(language, count, one, other);
    pseudo_render(language, &craft_i18n::fmt(&translated, args))
}

/// Translate and format a user-facing message in a specified language, falling back to English.
pub fn format_message_for(language: &str, source: &str, args: &[(&str, &str)]) -> String {
    let language = I18N.from_code(language).unwrap_or(Lang::EN);
    let Some(id) = source_id(source) else {
        return craft_i18n::fmt(source, args);
    };
    pseudo_render(language, &craft_i18n::fmt(lookup_id(language, id), args))
}

/// Translate a source string in a specified language, falling back to English.
pub fn translate_for(language: &str, source: &str) -> String {
    let language = I18N.from_code(language).unwrap_or(Lang::EN);
    source_id(source).map_or_else(|| source.to_string(), |id| lookup_id(language, id).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_languages_and_ltr_pseudo_locale_are_offered() {
        let languages = available_languages();
        let codes: Vec<&str> = languages.iter().map(|language| language.code.as_str()).collect();
        assert_eq!(codes, ["en", "ar", "de", "es", "fa", "fr", "he", "hi", "ja", "ko", "ru", "ur", "zh", "en-XA", "ar-XB"]);
        assert_eq!(languages[13].name, "English (Pseudo, LTR)");
        assert_eq!(languages[14].name, "Arabic (Pseudo, RTL)");
        for (code, name) in [
            ("ar", "العربية"),
            ("de", "Deutsch"),
            ("es", "Español"),
            ("fa", "فارسی"),
            ("fr", "Français"),
            ("he", "עברית"),
            ("hi", "हिन्दी"),
            ("ja", "日本語"),
            ("ko", "한국어"),
            ("ru", "Русский"),
            ("ur", "اردو"),
            ("zh", "中文（简体）"),
        ] {
            assert!(languages.iter().any(|language| language.code == code && language.name == name), "missing native locale name for {code}");
        }
        assert!(I18N.check_all().is_empty(), "invalid embedded catalog rows: {:?}", I18N.check_all());
    }

    #[test]
    fn spanish_catalog_translates_ui_messages_and_counts() {
        assert_eq!(normalize_preference("ES"), "es");
        assert_eq!(translate_for("es", "Interface language"), "Idioma de la interfaz");
        assert_eq!(translate_for("es", "Search all pages"), "Buscar en todas las páginas");
        assert_eq!(
            format_message_for("es", "Could not save the interface language: {error}", &[("error", "disk full")]),
            "No se ha podido guardar el idioma de la interfaz: disk full"
        );
        let spanish = I18N.from_code("es").unwrap_or(Lang::EN);
        assert_eq!(I18N.trn(spanish, 1, "{n} item", "{n} items"), "1 elemento");
        assert_eq!(I18N.trn(spanish, 2, "{n} item", "{n} items"), "2 elementos");
        assert!(I18N.check_all().is_empty(), "invalid embedded catalog rows: {:?}", I18N.check_all());
    }

    #[test]
    fn en_xa_is_a_normal_persistable_locale_with_pseudo_lookups() {
        assert_eq!(normalize_preference("EN-xa"), "en-XA");
        assert_eq!(resolve_preference("en-XA", &["fr-FR"]), "en-XA");
        assert_eq!(resolve_preference(AUTO, &["en-XA", "fr-FR"]), "fr");
        set_current("en-XA");
        assert_eq!(t("Saved"), "$$$Saved@@@");
        assert_eq!(
            format_message_for("en-XA", "{name} is not built yet. See the roadmap for what is coming.", &[("name", "Search")]),
            "$$$Search is not built yet. See the roadmap for what is coming.@@@"
        );
        assert_eq!(translate_for("en-XA", "Saved"), "$$$Saved@@@");
        let missing = ["missing ", "pseudo ", "key 21943"].concat();
        assert_eq!(translate_for("en-XA", &missing), missing);
        set_current("en");
    }

    #[test]
    fn ar_xb_keeps_source_words_and_adds_rtl_isolation() {
        assert_eq!(normalize_preference("AR-xb"), "ar-XB");
        assert_eq!(resolve_preference("ar-XB", &["en-US"]), "ar-XB");
        assert_eq!(resolve_preference(AUTO, &["ar-XB", "en-US"]), "en");
        set_current("ar-XB");
        assert_eq!(current_code(), "ar-XB");
        assert_eq!(t("Options"), "\u{2067}$$$Options@@@\u{2069}");
        assert_eq!(pseudo_unmarked(t("Options")), "Options");
        let missing = ["missing ", "pseudo ", "key 21943"].concat();
        assert_eq!(t(&missing), missing);
        set_current("en");
    }

    #[test]
    fn french_translates_the_pilot_and_missing_strings_fall_back_to_english() {
        assert_eq!(translate_for("fr", "Interface language"), "Langue de l’interface");
        assert_eq!(translate_for("fr", "Notebook title"), "Notebook title");
    }

    #[test]
    fn french_covers_ui_strings_visible_in_the_localization_screenshots() {
        for (source, expected) in [
            ("Search all pages", "Rechercher dans toutes les pages"),
            ("Add page", "Ajouter une page"),
            ("A new page in this section", "Nouvelle page dans cette section"),
            ("Drop a file onto the page to add it", "Déposez un fichier sur la page pour l’ajouter"),
            ("Saved", "Enregistré"),
            ("Editing...", "Modification..."),
            ("Tags", "Étiquettes"),
            ("NOTEBOOKS", "BLOCS-NOTES"),
            ("Ready", "Prêt"),
            ("Opened", "Page ouverte"),
            ("Default format", "Format par défaut"),
            ("New pages:", "Nouvelles pages :"),
        ] {
            assert_eq!(translate_for("fr", source), expected, "French catalog entry for {source}");
        }
        let french = I18N.from_code("fr").unwrap_or(Lang::EN);
        assert_eq!(I18N.trn(french, 2, "{n} notebook", "{n} notebooks"), "2 blocs-notes");
        assert_eq!(I18N.trn(french, 14, "{n} item", "{n} items"), "14 éléments");
    }

    #[test]
    fn pseudo_catalogs_mark_only_existing_rows_and_record_lookups() {
        set_current("en-XA");
        assert_eq!(t("Saved"), "$$$Saved@@@");
        let missing = ["missing ", "pseudo ", "key 21943"].concat();
        assert_eq!(t(&missing), missing);
        assert_eq!(pseudo_unmarked("prefix $$$marked@@@ suffix"), "prefix marked suffix");
        assert!(source_id("Saved").is_some_and(|id| pseudo_looked_up().contains(id)));
        assert!(!available_languages().iter().any(|language| language.code == "pseudo"));
        set_current("en");
    }

    #[test]
    fn pseudo_dynamic_does_not_wrap_an_already_translated_lookup_twice() {
        set_current("en-XA");
        assert_eq!(pseudo_dynamic(t("Ready")), "$$$Ready@@@");
        assert_eq!(pseudo_dynamic("unextracted runtime text"), "unextracted runtime text");
        set_current("en");
    }

    #[test]
    fn selecting_english_restores_source_text_after_pseudo_locale() {
        set_current("en-XA");
        assert_eq!(t("Saved"), "$$$Saved@@@");
        set_current("en");
        assert_eq!(t("Saved"), "Saved");
    }

    #[test]
    fn user_facing_message_lookup_formats_translated_placeholders() {
        assert_eq!(format_message_for("fr", "Interface language: {language}", &[("language", "English")]), "Langue de l’interface : English");
        assert_eq!(
            format_message_for("fr", "Could not save the interface language: {error}", &[("error", "disk full")]),
            "Impossible d’enregistrer la langue de l’interface : disk full"
        );
    }

    #[test]
    fn counts_use_the_shared_locale_plural_rules() {
        CURRENT.set(Lang::EN);
        assert_eq!(format_count("{n} item", "{n} items", 1), "1 item");
        assert_eq!(format_count("{n} item", "{n} items", 2), "2 items");
    }

    #[test]
    fn saved_language_preferences_are_canonical_and_reject_unknown_languages() {
        assert_eq!(normalize_preference("FR"), "fr");
        assert_eq!(normalize_preference("EN-xa"), "en-XA");
        assert_eq!(normalize_preference("en"), "en");
        assert_eq!(normalize_preference("xx"), AUTO);
    }

    #[test]
    fn automatic_language_uses_the_first_supported_system_locale_or_english() {
        assert_eq!(resolve_preference(AUTO, &["fr-CA", "en-US"]), "fr");
        assert_eq!(resolve_preference(AUTO, &["de-DE"]), "de");
        assert_eq!(resolve_preference(AUTO, &["xx-YY"]), "en");
        assert_eq!(resolve_preference("en", &["fr-FR"]), "en");
    }
}
