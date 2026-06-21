use core::fmt;

use musli_core::{Allocator, Decode, Decoder, Encode, Encoder};

/// A kind of data a [`RemoteSource`] can contribute during a layered sync.
///
/// During a sync the enabled remotes are visited in priority order and each
/// contributes the kinds it supports ([`RemoteSource::sync_kinds`]). A draft
/// tracks which kinds have already been contributed: an *exclusive* kind
/// ([`SyncKind::is_exclusive`]) is taken by the first source that provides it and
/// skipped by later layers, while a non-exclusive kind accumulates from every
/// source. Graphics always accumulate and are tracked separately.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode, serde::Serialize, serde::Deserialize,
)]
#[musli(crate = musli_core)]
#[serde(rename_all = "snake_case")]
pub enum SyncKind {
    /// Core metadata: title, overview, seasons and episode details.
    Base,
    /// Episode air dates (recorded as `episode_releases` and merged by priority).
    AirDate,
}

impl SyncKind {
    /// All sync kinds, in a stable order.
    pub const ALL: &[Self] = &[Self::Base, Self::AirDate];

    /// Whether only the first (highest-priority) source providing this kind
    /// contributes it. Non-exclusive kinds accumulate from every source.
    pub fn is_exclusive(&self) -> bool {
        matches!(self, Self::Base)
    }

    /// Human-readable label.
    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Base => "Base",
            Self::AirDate => "Air Date",
        }
    }

    /// The single bit representing this kind in a [`SyncKindSet`].
    pub fn bit(&self) -> u32 {
        match self {
            Self::Base => 1 << 0,
            Self::AirDate => 1 << 1,
        }
    }
}

/// A set of [`SyncKind`]s, stored compactly as a bitmask. Used both as a
/// source's capability ceiling ([`RemoteSource::default_sync_kinds`]) and as the
/// configured selection (global default or per-remote override).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncKindSet(u32);

impl SyncKindSet {
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline]
    pub fn from_kinds(kinds: impl IntoIterator<Item = SyncKind>) -> Self {
        let mut set = Self::empty();

        for k in kinds {
            set.0 |= k.bit();
        }

        set
    }

    /// Reconstruct from raw bits, masking off any unknown bits.
    pub fn from_bits(bits: u32) -> Self {
        let known: u32 = SyncKind::ALL.iter().map(SyncKind::bit).sum();
        Self(bits & known)
    }

    pub fn bits(&self) -> u32 {
        self.0
    }

    pub fn contains(&self, kind: SyncKind) -> bool {
        self.0 & kind.bit() != 0
    }

    pub fn insert(&mut self, kind: SyncKind) {
        self.0 |= kind.bit();
    }

    pub fn with(mut self, kind: SyncKind, on: bool) -> Self {
        if on {
            self.0 |= kind.bit();
        } else {
            self.0 &= !kind.bit();
        }

        self
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// The kinds present in both sets.
    pub fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// The kinds present, in [`SyncKind::ALL`] order.
    #[inline]
    pub fn iter(&self) -> SyncKindSetIter {
        SyncKindSetIter { base: self.0 }
    }
}

impl IntoIterator for SyncKindSet {
    type Item = SyncKind;
    type IntoIter = SyncKindSetIter;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An iterator over the kinds present in a [`SyncKindSet`].
pub struct SyncKindSetIter {
    base: u32,
}

impl Iterator for SyncKindSetIter {
    type Item = SyncKind;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.base == 0 {
            return None;
        }

        let bit = self.base.trailing_zeros();
        self.base &= !(1 << bit);

        match bit {
            0 => Some(SyncKind::Base),
            1 => Some(SyncKind::AirDate),
            _ => None,
        }
    }
}

impl FromIterator<SyncKind> for SyncKindSet {
    #[inline]
    fn from_iter<T: IntoIterator<Item = SyncKind>>(iter: T) -> Self {
        Self::from_kinds(iter)
    }
}

impl<M> Encode<M> for SyncKindSet {
    type Encode = Self;

    #[inline]
    fn encode<E>(&self, encoder: E) -> Result<(), E::Error>
    where
        E: Encoder<Mode = M>,
    {
        self.0.encode(encoder)
    }

    #[inline]
    fn as_encode(&self) -> &Self::Encode {
        self
    }
}

impl<'de, M, A> Decode<'de, M, A> for SyncKindSet
where
    A: Allocator,
{
    fn decode<D>(decoder: D) -> Result<Self, D::Error>
    where
        D: Decoder<'de, Mode = M, Allocator = A>,
    {
        Ok(Self::from_bits(u32::decode(decoder)?))
    }
}

impl serde::Serialize for SyncKindSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_seq(self.iter())
    }
}

impl<'de> serde::Deserialize<'de> for SyncKindSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SyncKindSetVisitor;

        impl<'de> serde::de::Visitor<'de> for SyncKindSetVisitor {
            type Value = SyncKindSet;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a sequence of sync-kind strings or a legacy integer bitmask")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<SyncKindSet, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut set = SyncKindSet::empty();

                while let Some(kind) = seq.next_element::<SyncKind>()? {
                    set.insert(kind);
                }

                Ok(set)
            }

            // Legacy format: the raw bitmask as an integer.
            fn visit_u64<E>(self, v: u64) -> Result<SyncKindSet, E> {
                Ok(SyncKindSet::from_bits(v as u32))
            }

            fn visit_i64<E>(self, v: i64) -> Result<SyncKindSet, E> {
                Ok(SyncKindSet::from_bits(v as u32))
            }
        }

        deserializer.deserialize_any(SyncKindSetVisitor)
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::FromColumn<'_> for SyncKindSet {
    type Type = ::sqll::ty::Integer;

    fn from_column(stmt: &::sqll::Statement, index: ::sqll::ty::Integer) -> ::sqll::Result<Self> {
        Ok(Self::from_bits(u32::from_column(stmt, index)?))
    }
}

#[cfg(feature = "sqll")]
impl ::sqll::BindValue for SyncKindSet {
    fn bind_value(&self, stmt: &mut ::sqll::Statement, index: ::sqll::Index) -> ::sqll::Result<()> {
        self.0.bind_value(stmt, index)
    }
}
