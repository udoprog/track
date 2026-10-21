//! The stored form of api's column types: each binds as, and reads back from,
//! exactly the value existing databases hold.

use core::fmt::Debug;

use anyhow::Result;
use api::{
    CreditKind, Date, ImageKind, ImageSource, PreferenceKey, ReleaseType, RemoteSource, ShowId,
    StringKind, ThemeType, Timestamp,
};
use sqll::{BindValue, Code, Connection, FromColumn};

/// Assert that `value` binds as `stored` and that `stored` reads back as
/// `value`.
fn round_trip<T, S>(c: &Connection, value: T, stored: S) -> Result<()>
where
    T: BindValue + for<'stmt> FromColumn<'stmt> + PartialEq + Debug,
    S: BindValue + for<'stmt> FromColumn<'stmt> + PartialEq + Debug,
{
    let mut stmt = c.prepare("SELECT ?")?;
    let bound = stmt.first::<S>((&value,))?;
    assert_eq!(bound.as_ref(), Some(&stored), "{value:?} is stored");

    let mut stmt = c.prepare("SELECT ?")?;
    let read = stmt.first::<T>((&stored,))?;
    assert_eq!(read.as_ref(), Some(&value), "{stored:?} reads back");
    Ok(())
}

/// Read `stored` as a `T`.
fn read<T>(c: &Connection, stored: impl BindValue) -> sqll::Result<Option<T>>
where
    T: for<'stmt> FromColumn<'stmt>,
{
    c.prepare("SELECT ?")?.first::<T>((stored,))
}

#[test]
fn ids_bit_cast_through_i64() -> Result<()> {
    let c = Connection::open_in_memory()?;
    round_trip(&c, ShowId::new(1), 1i64)?;
    round_trip(&c, ShowId::new(u64::MAX), -1i64)?;
    round_trip(&c, ShowId::new(1 << 63), i64::MIN)?;
    Ok(())
}

#[test]
fn remote_source() -> Result<()> {
    let c = Connection::open_in_memory()?;

    for (value, stored) in [
        (RemoteSource::Unknown, 0i64),
        (RemoteSource::Tvdb, 1),
        (RemoteSource::Tmdb, 2),
        (RemoteSource::Imdb, 3),
        (RemoteSource::Tvmaze, 4),
        (RemoteSource::Xem, 5),
        (RemoteSource::Anidb, 6),
        (RemoteSource::Scene, 7),
    ] {
        round_trip(&c, value, stored)?;
    }

    assert_eq!(read(&c, 99i64)?, Some(RemoteSource::Unknown));
    Ok(())
}

#[test]
fn image_kind() -> Result<()> {
    let c = Connection::open_in_memory()?;

    for (value, stored) in [
        (ImageKind::Unknown, 0i64),
        (ImageKind::Poster, 1),
        (ImageKind::Banner, 2),
        (ImageKind::Backdrop, 3),
        (ImageKind::Screenshot, 4),
        (ImageKind::Profile, 5),
    ] {
        round_trip(&c, value, stored)?;
    }

    assert_eq!(read(&c, 99i64)?, Some(ImageKind::Unknown));
    Ok(())
}

#[test]
fn string_kind() -> Result<()> {
    let c = Connection::open_in_memory()?;

    for (value, stored) in [
        (StringKind::Unknown, 0i64),
        (StringKind::Title, 1),
        (StringKind::Overview, 2),
        (StringKind::Character, 3),
        (StringKind::Job, 4),
        (StringKind::Department, 5),
    ] {
        round_trip(&c, value, stored)?;
    }

    assert_eq!(read(&c, 99i64)?, Some(StringKind::Unknown));
    Ok(())
}

#[test]
fn credit_kind() -> Result<()> {
    let c = Connection::open_in_memory()?;
    round_trip(&c, CreditKind::Cast, 1i64)?;
    round_trip(&c, CreditKind::Crew, 2i64)?;
    assert_eq!(read(&c, 0i64)?, Some(CreditKind::Cast));
    Ok(())
}

#[test]
fn image_source() -> Result<()> {
    let c = Connection::open_in_memory()?;
    round_trip(&c, ImageSource::Unknown, 0i64)?;
    round_trip(&c, ImageSource::Tvdb, 1i64)?;
    round_trip(&c, ImageSource::Tmdb, 2i64)?;
    assert_eq!(read(&c, 99i64)?, Some(ImageSource::Unknown));
    Ok(())
}

#[test]
fn release_type() -> Result<()> {
    let c = Connection::open_in_memory()?;

    for (value, stored) in [
        (ReleaseType::Unknown, 0i64),
        (ReleaseType::Premiere, 1),
        (ReleaseType::TheatricalLimited, 2),
        (ReleaseType::Theatrical, 3),
        (ReleaseType::Digital, 4),
        (ReleaseType::Physical, 5),
        (ReleaseType::Tv, 6),
    ] {
        round_trip(&c, value, stored)?;
        assert_eq!(i64::from(value.as_u32()), stored);
    }

    assert_eq!(read(&c, 99i64)?, Some(ReleaseType::Unknown));
    Ok(())
}

#[test]
fn theme_type() -> Result<()> {
    let c = Connection::open_in_memory()?;
    round_trip(&c, ThemeType::Dark, String::from("dark"))?;
    round_trip(&c, ThemeType::Light, String::from("light"))?;
    round_trip(&c, ThemeType::System, String::from("system"))?;

    let error = read::<ThemeType>(&c, "sepia").unwrap_err();
    assert_eq!(error.code(), Code::MISMATCH);
    Ok(())
}

#[test]
fn preference_key_binds_as_its_text() -> Result<()> {
    let c = Connection::open_in_memory()?;

    for &key in PreferenceKey::ALL {
        let mut stmt = c.prepare("SELECT ?")?;
        assert_eq!(
            stmt.first::<String>((key,))?.as_deref(),
            Some(key.as_str()),
            "{key:?} is stored"
        );
    }

    Ok(())
}

#[test]
fn out_of_range_times_are_mismatches() -> Result<()> {
    let c = Connection::open_in_memory()?;

    let error = read::<Timestamp>(&c, i64::MAX).unwrap_err();
    assert_eq!(error.code(), Code::MISMATCH);

    let error = read::<Date>(&c, 2026_13_40i64).unwrap_err();
    assert_eq!(error.code(), Code::MISMATCH);
    Ok(())
}
