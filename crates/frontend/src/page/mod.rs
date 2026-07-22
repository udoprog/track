//! Top-level pages, one per route, plus the dashboard's embedded calendar and
//! watch-next sections. Each page lives in its own module, named after the
//! component it exports, and is re-exported here so callers can refer to it as
//! `crate::page::<Name>`.

mod calendar;
mod dashboard;
mod media_list;
mod movie_detail;
mod person_detail;
mod person_list;
mod queue;
mod schedule_range;
mod search;
mod settings;
mod show_detail;
mod watch_next;

pub(crate) use self::calendar::Calendar;
pub(crate) use self::dashboard::Dashboard;
pub(crate) use self::media_list::MediaList;
pub(crate) use self::movie_detail::MovieDetail;
pub(crate) use self::person_detail::PersonDetail;
pub(crate) use self::person_list::PersonList;
pub(crate) use self::queue::Queue;
pub(crate) use self::schedule_range::ScheduleRange;
pub(crate) use self::search::Search;
pub(crate) use self::settings::Settings;
pub(crate) use self::show_detail::ShowDetail;
pub(crate) use self::watch_next::WatchNext;
