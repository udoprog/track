use core::fmt;

use crate::macros;

macros::define_code! {
    /// A language identified by its 3-letter ISO 639-3 code, stored as four
    /// bytes: the three ASCII letters followed by a `0` pad.
    ///
    /// The all-zero value is [`Language::DEFAULT`], a reference-time sentinel
    /// meaning "use the media's own default (original) language". It is never a
    /// real language and must never be persisted as data in the `strings` table.
    pub struct Language;
    /// Error produced when a string cannot be parsed as a [`Language`].
    pub struct ParseLanguageErr("invalid language code");
    /// Build a language from its raw representation.
    ///
    /// If the input is not a valid ISO 639-3 code, returns
    /// [`Language::DEFAULT`].
    ///
    /// ```
    /// use api::Language;
    ///
    /// assert_eq!(Language::new(b"eng"), Language::ENG);
    /// assert_eq!(Language::new(b"ENG"), Language::ENG);
    /// assert_eq!(Language::new(b"zzz"), Language::DEFAULT);
    /// ```
    const fn new valid by iso639::is_valid_id;
    case to_ascii_lowercase, is_ascii_lowercase;
    expecting "a 2- or 3-letter ISO 639 language code or 'default'";
    stored error "invalid stored language code";
}

impl Language {
    /// English (`eng`).
    pub const ENG: Language = Language::new(b"eng");

    /// Filter the current language, returning `DEFAULT` if the predicate
    /// returns false.
    #[inline]
    pub fn filter(self, f: impl FnOnce(Self) -> bool) -> Self {
        if f(self) { self } else { Self::DEFAULT }
    }

    /// Build from a 2- or 3-letter ISO 639 code (case-insensitive). A 2-letter
    /// code is resolved to its 3-letter form via the `iso639` data. An empty
    /// string or `"default"` maps to [`Language::DEFAULT`]. Returns `None` for
    /// anything else.
    pub fn from_iso(code: &str) -> Option<Self> {
        let code = code.trim();

        if code.is_empty() || code.eq_ignore_ascii_case("default") {
            return Some(Self::DEFAULT);
        }

        let mut bytes = [0u8; 4];
        let lower = Self::normalize(&mut bytes, code)?;

        let bytes = match lower.len() {
            2 => {
                let id = iso639::by_part1(lower)?.id;

                let &[a, b, c] = id.as_bytes() else {
                    return None;
                };

                [a, b, c, 0]
            }
            3 if iso639::is_valid_id(lower.as_bytes()) => bytes,
            _ => return None,
        };

        Some(Self(bytes))
    }

    /// The static [`Language`] structure associated with this code, or `None`
    /// for [`Language::DEFAULT`].
    ///
    /// [`Language`]: iso639::Language
    pub fn to_iso(&self) -> Option<&'static iso639::Language> {
        if self.is_default() {
            return None;
        }

        iso639::by_id(self.as_raw_code())
    }

    /// The 3-letter ISO 639-3 code, if one exists or `None` for `DEFAULT`.
    pub fn to_id(&self) -> Option<&'static str> {
        self.to_iso().map(|e| e.id)
    }

    /// The 2-letter ISO 639-1 code, if one exists (`None` for `DEFAULT` or
    /// codes without a 2-letter form). Used for remotes that key on ISO 639-1.
    pub fn to_part1(&self) -> Option<&'static str> {
        self.to_iso()?.part1
    }
}

impl fmt::Debug for Language {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
