//! Shared UI building blocks: small reusable widgets, pickers, and editors used
//! across the frontend's pages. Each component lives in its own module, named
//! after the component it exports, and is re-exported here so callers can refer
//! to it as `crate::ui::<Name>`.

mod button;
mod confirm_danger;
mod context_menu;
mod country_picker;
mod detail_hero;
mod detail_skeleton;
mod duration_input;
mod episode_cache_modal;
mod episode_picker;
mod error_box;
mod filters_editor;
pub(crate) mod focus;
mod form_row;
mod graphics_source_filter;
mod image;
mod image_gallery;
mod language_modal;
mod language_picker;
mod link;
mod mark_time_menu;
mod media_kind_toggle;
mod media_settings_modal;
mod modal;
mod outline;
mod pagination_buttons;
mod release_modal;
mod remote_editor;
mod reorder;
mod secret_input;
mod skeleton;
mod sort_menu;
mod sync_kinds_editor;
mod sync_languages_editor;
mod tracked;
mod translated_text;
mod translations_modal;

pub(crate) use self::button::{Button, Variant};
pub(crate) use self::confirm_danger::ConfirmDanger;
pub(crate) use self::context_menu::ContextMenu;
pub(crate) use self::country_picker::CountryPicker;
pub(crate) use self::detail_hero::DetailHero;
pub(crate) use self::detail_skeleton::DetailSkeleton;
pub(crate) use self::duration_input::DurationInput;
pub(crate) use self::episode_cache_modal::EpisodeCacheModal;
pub(crate) use self::episode_picker::EpisodePicker;
pub(crate) use self::error_box::ErrorBox;
pub(crate) use self::filters_editor::{
    AIR_DATE_KINDS, AIR_DATE_SOURCES, FiltersEditor, RELEASE_KINDS, RELEASE_SOURCES,
};
pub(crate) use self::form_row::FormRow;
pub(crate) use self::graphics_source_filter::GraphicsSourceFilter;
pub(crate) use self::image::Image;
pub(crate) use self::image_gallery::{ImageGallery, ImageItem};
pub(crate) use self::language_modal::{LanguageModal, TopLanguages, locale_label};
pub(crate) use self::language_picker::LanguagePicker;
pub(crate) use self::link::Link;
pub(crate) use self::mark_time_menu::{MarkTimeMenu, TimePreset};
pub(crate) use self::media_kind_toggle::MediaKindToggle;
pub(crate) use self::media_settings_modal::{MediaSettingsModal, SettingsTarget};
pub(crate) use self::modal::Modal;
pub(crate) use self::outline::{Outline, OutlineControl, OutlineEntry, OutlineHandle};
pub(crate) use self::pagination_buttons::PaginationButtons;
pub(crate) use self::release_modal::{ReleaseModal, ReleaseTarget};
pub(crate) use self::remote_editor::{RemoteEditor, RemoteSourceKind};
pub(crate) use self::reorder::{DragHandle, Reorder};
pub(crate) use self::secret_input::SecretInput;
pub(crate) use self::skeleton::Skeleton;
pub(crate) use self::sort_menu::SortMenu;
pub(crate) use self::sync_kinds_editor::SyncKindsEditor;
pub(crate) use self::sync_languages_editor::SyncLanguagesEditor;
pub(crate) use self::tracked::Tracked;
pub(crate) use self::translated_text::TranslatedText;
pub(crate) use self::translations_modal::TranslationsModal;

pub(crate) const SEARCH: &str = "Search…";
