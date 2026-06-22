use core::fmt;
use core::str::FromStr;

#[cfg(feature = "yew")]
use implicit_clone::unsync::IString;
use musli_core::{Allocator, Context, Decode, Decoder, Encode, Encoder};
#[cfg(feature = "yew")]
use yew::html::IntoPropValue;
#[cfg(feature = "yew")]
use yew::virtual_dom::VNode;

use crate::{Country, Language};

/// A locale: the concatenation of a [`Language`] and a [`Country`], for example
/// `en-US` or `pt-BR`.
///
/// The 64-bit integer representation keeps the language in the **low 32 bits**
/// and the country in the **high 32 bits**. This is deliberately backwards
/// compatible with the old bare-[`Language`] integer encoding: a value whose
/// high bits are zero decodes to that language with [`Country::DEFAULT`], so no
/// data migration is needed. The string form is likewise compatible — an old
/// `"eng"` parses as a language-only locale.
///
/// [`Locale::DEFAULT`] (both components default) is the reference-time sentinel
/// meaning "use the media's own default (original) language".
#[derive(Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Locale {
    language: Language,
    country: Country,
}

/// Error produced when a string cannot be parsed as a [`Locale`].
#[derive(Debug)]
pub struct ParseLocaleErr;

impl fmt::Display for ParseLocaleErr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid locale code")
    }
}

impl core::error::Error for ParseLocaleErr {}

impl Locale {
    /// Sentinel meaning "use the media's own default (original) language".
    pub const DEFAULT: Locale = Locale {
        language: Language::DEFAULT,
        country: Country::DEFAULT,
    };

    /// Construct a locale from its language and country components.
    #[inline]
    pub const fn new(language: Language, country: Country) -> Self {
        Self { language, country }
    }

    /// The language component.
    #[inline]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// The country component.
    #[inline]
    pub const fn country(&self) -> Country {
        self.country
    }

    /// Whether both components are [`Language::DEFAULT`] / [`Country::DEFAULT`].
    #[inline]
    pub const fn is_default(&self) -> bool {
        self.language.is_default() && self.country.is_default()
    }

    /// The flag CSS class to render for this locale. When a country is set, its
    /// flag is used (and nothing else); otherwise the language's own default
    /// flag is used. Returns `None` when no flag is available.
    pub fn flag(&self) -> Option<&'static str> {
        if !self.country.is_default() {
            return self
                .country
                .to_iso()
                .filter(|c| c.has_flag)
                .map(|c| c.alpha2);
        }

        self.language.to_iso().and_then(|l| l.flag)
    }

    /// Unwrap the current locale or fall back to `other` when the language
    /// component is [`Language::DEFAULT`] (the "use original" sentinel). Mirrors
    /// [`Language::or`].
    #[inline]
    pub fn or(self, other: Self) -> Self {
        if self.language.is_default() {
            other
        } else {
            self
        }
    }

    /// The 64-bit integer representation: country in the high 32 bits, language
    /// in the low 32 bits.
    #[inline]
    pub const fn to_u64(self) -> u64 {
        let language = u32::from_be_bytes(self.language.0) as u64;
        let country = u32::from_be_bytes(self.country.0) as u64;
        (country << 32) | language
    }

    /// Rebuild a locale from its 64-bit integer representation. The inverse of
    /// [`Locale::to_u64`].
    #[inline]
    pub const fn from_u64(value: u64) -> Self {
        Self {
            language: Language((value as u32).to_be_bytes()),
            country: Country(((value >> 32) as u32).to_be_bytes()),
        }
    }

    /// Build from a `language[-country]` ISO code (case-insensitive), e.g.
    /// `"en-US"`, `"eng-US"` or `"en"`. The language part is resolved via the
    /// `iso639` data; a country part, when present, via the `iso3166` data.
    ///
    /// Unlike [`Language::from_iso`]/[`Country::from_iso`] this **never** parses
    /// the `"default"` sentinel (nor an empty string): a locale must name a real
    /// language, and a country segment, when present, must name a real country.
    pub fn from_iso(code: &str) -> Option<Self> {
        let code = code.trim();

        if code.is_empty() {
            return None;
        }

        let (lang, country) = match code.split_once('-') {
            Some((lang, country)) => (lang, Some(country)),
            None => (code, None),
        };

        let language = Language::from_iso(lang)?;

        // Reject "default"/empty (and anything that resolves to the sentinel).
        if language.is_default() {
            return None;
        }

        let country = match country {
            Some(country) => {
                let country = Country::from_iso(country)?;

                if country.is_default() {
                    return None;
                }

                country
            }
            None => Country::DEFAULT,
        };

        Some(Self { language, country })
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_default() {
            return f.write_str("default");
        }

        // Prefer the 2-letter ISO 639-1 (part1) code, falling back to the
        // 3-letter ISO 639-3 form (the language's own Display).
        match self.language.to_part1() {
            Some(part1) => f.write_str(part1)?,
            None => write!(f, "{}", self.language)?,
        }

        if !self.country.is_default() {
            write!(f, "-{}", self.country)?;
        }

        Ok(())
    }
}

impl fmt::Debug for Locale {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl FromStr for Locale {
    type Err = ParseLocaleErr;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_iso(s).ok_or(ParseLocaleErr)
    }
}

impl serde::Serialize for Locale {
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Locale {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = Locale;

            #[inline]
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 'language[-country]' locale code or 'default'")
            }

            #[inline]
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                // The all-default sentinel round-trips as "default" — the only
                // place that string is accepted.
                if v.trim().eq_ignore_ascii_case("default") {
                    return Ok(Locale::DEFAULT);
                }

                v.parse().map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl<M> Encode<M> for Locale {
    type Encode = Self;

    #[inline]
    fn encode<E>(&self, encoder: E) -> Result<(), E::Error>
    where
        E: Encoder<Mode = M>,
    {
        encoder.collect_string(self)
    }

    #[inline]
    fn as_encode(&self) -> &Self::Encode {
        self
    }
}

impl<'de, M, A> Decode<'de, M, A> for Locale
where
    A: Allocator,
{
    #[inline]
    fn decode<D>(decoder: D) -> Result<Self, D::Error>
    where
        D: Decoder<'de, Mode = M, Allocator = A>,
    {
        let cx = decoder.cx();
        decoder.decode_unsized(|s: &str| {
            // "default" round-trips to the sentinel (see the serde impl).
            if s.trim().eq_ignore_ascii_case("default") {
                return Ok(Locale::DEFAULT);
            }

            s.parse::<Locale>().map_err(cx.map())
        })
    }

    const IS_BITWISE_DECODE: bool = false;
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for Locale {
    type Type = ::sqll::ty::Integer;

    #[inline]
    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        let value = i64::from_column(stmt, index)?;
        Ok(Locale::from_u64(value.cast_unsigned()))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for Locale {
    #[inline]
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        self.to_u64().cast_signed().bind_value(stmt, index)
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<VNode> for Locale {
    #[inline]
    fn into_prop_value(self) -> VNode {
        self.to_string().into()
    }
}

#[cfg(feature = "yew")]
impl IntoPropValue<Option<IString>> for Locale {
    #[inline]
    fn into_prop_value(self) -> Option<IString> {
        if self.is_default() {
            None
        } else {
            Some(self.to_string().into())
        }
    }
}
