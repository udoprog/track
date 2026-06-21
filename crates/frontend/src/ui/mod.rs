//! Shared UI building blocks: small reusable widgets, pickers, and editors used
//! across the frontend's pages. Each component lives in a focused submodule and
//! is re-exported here so callers can refer to it as `crate::ui::<Name>`.

mod common;
mod country;
mod episode_picker;
mod filters;
mod language;
mod media_settings;
mod remote;
mod secret;

pub(crate) use self::common::{
    ConfirmDanger, ErrorBox, Loading, MediaKindToggle, PaginationButtons, Tracked,
};
pub(crate) use self::country::CountryPicker;
pub(crate) use self::episode_picker::EpisodePicker;
pub(crate) use self::filters::{AirDateFiltersEditor, ReleaseFiltersEditor, SyncKindsEditor};
pub(crate) use self::language::{LanguagePicker, SyncLanguagesEditor, TopLanguages};
pub(crate) use self::media_settings::MediaSettingsModal;
pub(crate) use self::remote::{RemoteEditor, RemoteSourceKind};
pub(crate) use self::secret::SecretInput;

pub(crate) const MDASH: &str = "—";
pub(crate) const LOADING: &str = "Loading…";
pub(crate) const SEARCH: &str = "Search…";
pub(crate) const DOT: &str = "•";
