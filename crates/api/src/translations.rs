use core::fmt;

use std::collections::HashMap;

use musli_core::{Allocator, Decode, Decoder, Encode, Encoder};

use crate::{Language, Locale, StringKind};

/// A single localized string for one `(kind, locale)`.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
#[musli(crate = musli_core)]
struct Entry {
    kind: StringKind,
    locale: Locale,
    text: String,
}

/// All localized strings for a single entity (show, season, episode or movie),
/// plus the locales needed to resolve them.
///
/// Strings are held in a flat `entries` array; `by_locale` and `by_language`
/// index into it for exact and country-relaxed lookups. [`Translations::get`]
/// resolves using the entity's configured display [`locale`](Self::locale),
/// falling back through the same language (any country), then the entity's
/// [`default`](Self::default) (original) language, then any available locale -
/// so a plain `get(StringKind::Title)` (or [`title`](Self::title)) resolves the
/// best title without the caller specifying any locale.
///
/// Built incrementally with [`new`](Self::new) + [`insert`](Self::insert):
/// inserting a `(kind, locale)` that already exists replaces it (no duplicate
/// entry). The two indexes are derived data - they are not serialized and are
/// rebuilt on decode.
#[derive(Clone, Default, PartialEq)]
pub struct Translations {
    /// The preferred display locale, already resolved against the global config.
    locale: Locale,
    /// Every string, in insertion order.
    entries: Vec<Entry>,
    /// Exact `(kind, locale)` -> index into `entries`.
    by_locale: HashMap<(StringKind, Locale), usize>,
    /// `(kind, language)` -> index into `entries`, ignoring country. Last write
    /// wins when several countries share a language.
    by_language: HashMap<(StringKind, Language), usize>,
}

impl Translations {
    /// Create an empty set for the configured display `locale` and the entity's
    /// `default` (original) language. Fill it with [`insert`](Self::insert).
    pub fn new(locale: Locale) -> Self {
        Self {
            locale,
            entries: Vec::new(),
            by_locale: HashMap::new(),
            by_language: HashMap::new(),
        }
    }

    /// Add a string, replacing any existing entry with the same `(kind, locale)`
    /// (the existing buffer is reused, so duplicates are deduplicated rather than
    /// appended).
    pub fn insert(&mut self, kind: StringKind, locale: Locale, text: &str) {
        if let Some(&idx) = self.by_locale.get(&(kind, locale)) {
            let entry = &mut self.entries[idx];
            entry.text.clear();
            entry.text.push_str(text);
            self.by_language.insert((kind, locale.language()), idx);
            return;
        }

        let idx = self.entries.len();
        self.entries.push(Entry {
            kind,
            locale,
            text: text.to_owned(),
        });
        self.by_locale.insert((kind, locale), idx);
        self.by_language.insert((kind, locale.language()), idx);
    }

    /// The configured display locale.
    #[inline]
    pub fn locale(&self) -> Locale {
        self.locale
    }

    /// Whether no strings are stored at all.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The resolved title, equivalent to `get(StringKind::Title)`.
    #[inline]
    pub fn title(&self) -> Option<&str> {
        self.get(StringKind::Title)
    }

    /// The resolved overview, equivalent to `get(StringKind::Overview)`.
    #[inline]
    pub fn overview(&self) -> Option<&str> {
        self.get(StringKind::Overview)
    }

    /// Resolve a string of `kind` using the configured locale and the entity's
    /// default language as the fallback. See [`Translations::get_with`].
    #[inline]
    pub fn get(&self, kind: StringKind) -> Option<&str> {
        self.get_with(kind, self.locale)
    }

    /// Resolve a string of `kind`, preferring `locale` then `fallback`. A
    /// [`Locale::DEFAULT`] `locale`/`fallback` resolves to the entity's default
    /// language. Resolution order: exact `locale`, same language as `locale`
    /// (any country), exact `fallback`, same language as `fallback`, the
    /// entity's default language, then any available string of `kind`.
    pub fn get_with(&self, kind: StringKind, locale: Locale) -> Option<&str> {
        if let Some(string) = self.exact(kind, locale) {
            return Some(string);
        };

        self.by_language(kind, locale)
    }

    /// Every stored string of `kind`, across all locales. Used for cross-locale
    /// filtering and listing alternate titles.
    pub fn texts(&self, kind: StringKind) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .filter(move |e| e.kind == kind)
            .map(|e| e.text.as_str())
    }

    fn exact(&self, kind: StringKind, locale: Locale) -> Option<&str> {
        let &idx = self.by_locale.get(&(kind, locale))?;
        Some(self.entries[idx].text.as_str())
    }

    fn by_language(&self, kind: StringKind, locale: Locale) -> Option<&str> {
        let language = locale.language();

        if language.is_default() {
            return None;
        }

        let &idx = self.by_language.get(&(kind, language))?;
        Some(self.entries[idx].text.as_str())
    }
}

/// The owned wire form of [`Translations`]: only the persisted fields. The
/// lookup indexes are derived and rebuilt on decode, so they are never stored.
#[derive(Decode)]
#[musli(crate = musli_core)]
struct TranslationsStorage {
    locale: Locale,
    entries: Vec<Entry>,
}

/// The borrowed counterpart of [`TranslationsStorage`] used for encoding without
/// cloning `entries`. A derived struct encodes its fields as a length-prefixed
/// sequence, and `&[Entry]` encodes identically to `Vec<Entry>`, so this is
/// wire-compatible with `TranslationsStorage`.
#[derive(Encode)]
#[musli(crate = musli_core)]
struct TranslationsStorageRef<'a> {
    locale: Locale,
    entries: &'a [Entry],
}

impl<M> Encode<M> for Translations
where
    for<'a> TranslationsStorageRef<'a>: Encode<M>,
{
    type Encode = Self;

    #[inline]
    fn encode<E>(&self, encoder: E) -> Result<(), E::Error>
    where
        E: Encoder<Mode = M>,
    {
        TranslationsStorageRef {
            locale: self.locale,
            entries: &self.entries,
        }
        .encode(encoder)
    }

    #[inline]
    fn as_encode(&self) -> &Self::Encode {
        self
    }
}

impl<'de, M, A> Decode<'de, M, A> for Translations
where
    A: Allocator,
    TranslationsStorage: Decode<'de, M, A>,
{
    #[inline]
    fn decode<D>(decoder: D) -> Result<Self, D::Error>
    where
        D: Decoder<'de, Mode = M, Allocator = A>,
    {
        let storage = TranslationsStorage::decode(decoder)?;

        let mut translations = Translations::new(storage.locale);
        translations.entries = storage.entries;

        for (index, entry) in translations.entries.iter().enumerate() {
            translations
                .by_locale
                .insert((entry.kind, entry.locale), index);

            translations
                .by_language
                .insert((entry.kind, entry.locale.language()), index);
        }

        Ok(translations)
    }

    const IS_BITWISE_DECODE: bool = false;
}

impl fmt::Debug for Translations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Translations")
            .field("locale", &self.locale)
            .field("entries", &self.entries)
            .finish()
    }
}
