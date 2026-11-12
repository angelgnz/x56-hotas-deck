//! Internationalization and Localization subsystem using Project Fluent.
//!
//! Provides compile-time embedded Fluent bundles for `en-US` (Primary Default)
//! and `es-ES` (Spanish), dynamic language switching at runtime, and automatic
//! detection of the user's system locale.

use std::borrow::Cow;
use std::sync::{Arc, OnceLock, RwLock};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use serde::{Deserialize, Serialize};
use unic_langid::langid;

const FTL_EN: &str = include_str!("../locales/en-US/main.ftl");
const FTL_ES: &str = include_str!("../locales/es-ES/main.ftl");

/// Supported localization languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    EnUs,
    EsEs,
}

impl Language {
    /// Returns the BCP 47 language code tag.
    #[inline]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::EsEs => "es-ES",
        }
    }

    /// Returns the two-letter language code (e.g., "en", "es").
    #[inline]
    pub const fn short_code(&self) -> &'static str {
        match self {
            Self::EnUs => "en",
            Self::EsEs => "es",
        }
    }

    /// Returns the human-readable native language name.
    #[inline]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::EnUs => "English",
            Self::EsEs => "Español",
        }
    }

    /// Parses a language code or system locale string (e.g. "es", "es_ES.UTF-8", "en_US").
    pub fn from_locale_str(s: &str) -> Self {
        let clean = s.to_ascii_lowercase();
        if clean.starts_with("es") {
            Self::EsEs
        } else {
            Self::EnUs
        }
    }

    /// Detects the user's preferred language from environment variables (`LC_ALL`, `LC_MESSAGES`, `LANG`).
    pub fn detect_system_language() -> Self {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                if !val.is_empty() {
                    return Self::from_locale_str(&val);
                }
            }
        }
        Self::EnUs
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Thread-safe manager holding pre-compiled Fluent bundles and active language state.
pub struct I18n {
    en_bundle: FluentBundle<FluentResource>,
    es_bundle: FluentBundle<FluentResource>,
    current_lang: Arc<RwLock<Language>>,
}

impl I18n {
    /// Initializes a new `I18n` instance with embedded FTL resources.
    pub fn new() -> Self {
        let initial_lang = Language::detect_system_language();

        // 1. Build en-US Bundle
        let mut en_bundle = FluentBundle::new_concurrent(vec![langid!("en-US")]);
        en_bundle.set_use_isolating(false);
        let en_res = match FluentResource::try_new(FTL_EN.to_string()) {
            Ok(r) => r,
            Err((res, errs)) => {
                log::error!("Failed to parse en-US Fluent resource: {:?}", errs);
                res
            }
        };
        if let Err(errs) = en_bundle.add_resource(en_res) {
            log::error!("Failed to add en-US resource to bundle: {:?}", errs);
        }

        // 2. Build es-ES Bundle
        let mut es_bundle = FluentBundle::new_concurrent(vec![langid!("es-ES")]);
        es_bundle.set_use_isolating(false);
        let es_res = match FluentResource::try_new(FTL_ES.to_string()) {
            Ok(r) => r,
            Err((res, errs)) => {
                log::error!("Failed to parse es-ES Fluent resource: {:?}", errs);
                res
            }
        };
        if let Err(errs) = es_bundle.add_resource(es_res) {
            log::error!("Failed to add es-ES resource to bundle: {:?}", errs);
        }

        Self {
            en_bundle,
            es_bundle,
            current_lang: Arc::new(RwLock::new(initial_lang)),
        }
    }

    /// Returns the currently active language.
    pub fn current_language(&self) -> Language {
        self.current_lang
            .read()
            .map(|l| *l)
            .unwrap_or(Language::EnUs)
    }

    /// Sets the active language.
    pub fn set_language(&self, lang: Language) {
        if let Ok(mut lock) = self.current_lang.write() {
            *lock = lang;
        }
    }

    /// Toggles between English and Spanish.
    pub fn toggle_language(&self) -> Language {
        let next = match self.current_language() {
            Language::EnUs => Language::EsEs,
            Language::EsEs => Language::EnUs,
        };
        self.set_language(next);
        next
    }

    /// Translates a message key using the active language, falling back to English, then to the key itself.
    pub fn tr(&self, id: &str) -> String {
        self.tr_with_args(id, &[])
    }

    /// Translates a message key with formatted dynamic arguments.
    pub fn tr_with_args(&self, id: &str, args: &[(&str, &str)]) -> String {
        let lang = self.current_language();

        let fluent_args = if args.is_empty() {
            None
        } else {
            let mut fa = FluentArgs::new();
            for &(k, v) in args {
                fa.set(k, FluentValue::from(v));
            }
            Some(fa)
        };

        // Try active bundle first
        if let Some(res) = self.format_bundle(lang, id, fluent_args.as_ref()) {
            return res;
        }

        // Fallback to English if active language was Spanish and key was missing
        if lang != Language::EnUs {
            if let Some(res) = self.format_bundle(Language::EnUs, id, fluent_args.as_ref()) {
                return res;
            }
        }

        // Final fallback: return the raw message ID
        id.to_string()
    }

    fn format_bundle(
        &self,
        lang: Language,
        id: &str,
        args: Option<&FluentArgs<'_>>,
    ) -> Option<String> {
        let bundle = match lang {
            Language::EnUs => &self.en_bundle,
            Language::EsEs => &self.es_bundle,
        };

        let message = bundle.get_message(id)?;
        let pattern = message.value()?;
        let mut errors = vec![];
        let value = bundle.format_pattern(pattern, args, &mut errors);

        if !errors.is_empty() {
            log::warn!("Formatting warnings for key '{}': {:?}", id, errors);
        }

        Some(match value {
            Cow::Borrowed(s) => s.to_string(),
            Cow::Owned(s) => s,
        })
    }
}

impl Default for I18n {
    fn default() -> Self {
        Self::new()
    }
}

static GLOBAL_I18N: OnceLock<I18n> = OnceLock::new();

/// Returns a reference to the global singleton `I18n` instance.
pub fn global() -> &'static I18n {
    GLOBAL_I18N.get_or_init(I18n::new)
}

/// Translates a message ID using the global localization manager.
#[inline]
pub fn tr(id: &str) -> String {
    global().tr(id)
}

/// Translates a message ID with dynamic arguments using the global localization manager.
#[inline]
pub fn tr_with_args(id: &str, args: &[(&str, &str)]) -> String {
    global().tr_with_args(id, args)
}

/// Returns the currently active language.
#[inline]
pub fn current_language() -> Language {
    global().current_language()
}

/// Sets the active language globally.
#[inline]
pub fn set_language(lang: Language) {
    global().set_language(lang);
}

/// Toggles between English and Spanish globally and returns the new language.
#[inline]
pub fn toggle_language() -> Language {
    global().toggle_language()
}

/// Ergonomic macro for Project Fluent translation.
///
/// # Examples
/// ```rust
/// use x56_core::t;
///
/// let title = t!("app-title");
/// let user_msg = t!("tray-profile", "name" => "COMBAT");
/// ```
#[macro_export]
macro_rules! t {
    ($id:expr) => {
        $crate::i18n::tr($id)
    };
    ($id:expr, $( $key:expr => $val:expr ),* $(,)?) => {
        $crate::i18n::tr_with_args($id, &[ $( ($key, $val) ),* ])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i18n_fallback_and_translation() {
        let i18n = I18n::new();

        // 1. English translations
        i18n.set_language(Language::EnUs);
        assert_eq!(i18n.tr("nav-curves"), "Response Curves");
        assert_eq!(
            i18n.tr_with_args("tray-profile", &[("name", "ALPHA")]),
            "Active profile: ALPHA"
        );

        // 2. Spanish translations
        i18n.set_language(Language::EsEs);
        assert_eq!(i18n.tr("nav-curves"), "Curvas de Respuesta");
        assert_eq!(
            i18n.tr_with_args("tray-profile", &[("name", "ALPHA")]),
            "Perfil activo: ALPHA"
        );

        // 3. Fallback for non-existent key returns key itself
        assert_eq!(i18n.tr("non-existent-key-123"), "non-existent-key-123");
    }

    #[test]
    fn test_language_toggle() {
        let i18n = I18n::new();
        i18n.set_language(Language::EnUs);
        assert_eq!(i18n.toggle_language(), Language::EsEs);
        assert_eq!(i18n.toggle_language(), Language::EnUs);
    }
}
