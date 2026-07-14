use yew::prelude::*;

use crate::background::Background;
use crate::error::Error;
use crate::router::{DashboardQuery, DashboardView, MediaSelection, Route, Router};
use crate::ui::MediaKindToggle;

use super::{Calendar, ScheduleRange, WatchNext};

pub(crate) struct Dashboard {
    background: Background,
    router: Router,
}

pub(crate) enum Msg {
    SetView(DashboardView),
    SetPage(usize),
    ClampPage(usize),
    SetWeek(i32),
    SetWeekStart(bool),
    ResetSchedule,
    SetRange(i32),
    SetSelection(MediaSelection),
}

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) page: usize,
    /// Schedule window offset from the current week, in weeks.
    pub(crate) week: i32,
    /// Mobile-only: reveal the past days of the current week.
    pub(crate) week_start: bool,
    /// Upcoming-days strip start offset from today, in days.
    pub(crate) range: i32,
    /// Which tabbed view is shown.
    pub(crate) view: DashboardView,
    /// Which media kinds the tabs show.
    pub(crate) selection: MediaSelection,
}

impl Component for Dashboard {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let (background, _) = ctx
            .link()
            .context::<Background>(Callback::noop())
            .expect("Expected background handle in context");

        let (router, _) = ctx
            .link()
            .context::<Router>(Callback::noop())
            .expect("Expected router in context");

        Self { background, router }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match self.try_update(ctx, msg) {
            Ok(render) => render,
            Err(e) => {
                self.background.error(e);
                false
            }
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.background.title(Some("Dashboard".to_string()));
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.background.title(None);
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let view = ctx.props().view;
        let selection = ctx.props().selection;

        let tab = |v: DashboardView,
                   icon: &'static str,
                   label: &'static str,
                   fill: Option<&'static str>| {
            let onclick = link.callback(move |_| Msg::SetView(v));
            html! {
                <span class={classes!("input-text", "has-text", fill, (view == v).then_some("selected"))} {onclick}>
                    <span class={classes!("icon", icon)} />
                    {label}
                </span>
            }
        };

        html! {
            <>
                <div class="desktop-center desktop-row mobile-column">
                    <div class="input-group">
                        { tab(DashboardView::WatchNext, "forward", "What's Next", Some("mobile-fill")) }
                        { tab(DashboardView::Upcoming, "calendar-days", "Upcoming", None) }
                        { tab(DashboardView::Schedule, "calendar", "Schedule", None) }
                    </div>

                    <div class="input-group">
                        <MediaKindToggle selection={selection} on_change={link.callback(Msg::SetSelection)} />
                    </div>
                </div>

                <div class="column">
                    { match view {
                        DashboardView::WatchNext => html! {
                            <WatchNext
                                page={ctx.props().page}
                                selection={selection}
                                on_set_page={link.callback(Msg::SetPage)}
                                on_clamp_page={link.callback(Msg::ClampPage)}
                            />
                        },
                        DashboardView::Upcoming => html! {
                            <ScheduleRange
                                day_offset={ctx.props().range}
                                selection={selection}
                                on_set_range={link.callback(Msg::SetRange)}
                            />
                        },
                        DashboardView::Schedule => html! {
                            <Calendar
                                week_offset={ctx.props().week}
                                week_start={ctx.props().week_start}
                                selection={selection}
                                on_set_week={link.callback(Msg::SetWeek)}
                                on_set_week_start={link.callback(Msg::SetWeekStart)}
                                on_reset={link.callback(|()| Msg::ResetSchedule)}
                            />
                        },
                    } }
                </div>
            </>
        }
    }
}

impl Dashboard {
    /// The current dashboard query reconstructed from props, so navigation can
    /// override a single field while preserving the rest.
    fn dashboard_query(&self, ctx: &Context<Self>) -> DashboardQuery {
        DashboardQuery {
            page: ctx.props().page,
            week: ctx.props().week,
            week_start: ctx.props().week_start,
            range: ctx.props().range,
            view: ctx.props().view,
            selection: ctx.props().selection,
        }
    }

    fn try_update(&mut self, ctx: &Context<Self>, msg: Msg) -> Result<bool, Error> {
        match msg {
            Msg::SetView(view) => {
                self.router.push(Route::Dashboard(DashboardQuery {
                    view,
                    ..self.dashboard_query(ctx)
                }));
                Ok(false)
            }
            Msg::SetPage(page) => {
                self.router.push(Route::Dashboard(DashboardQuery {
                    page,
                    ..self.dashboard_query(ctx)
                }));
                Ok(false)
            }
            Msg::ClampPage(page) => {
                // Replace rather than push: this is a URL correction, not a
                // navigation, so it should not leave a back-button target.
                self.router.replace(Route::Dashboard(DashboardQuery {
                    page,
                    ..self.dashboard_query(ctx)
                }));
                Ok(false)
            }
            Msg::SetWeek(week) => {
                self.router.push(Route::Dashboard(DashboardQuery {
                    week,
                    ..self.dashboard_query(ctx)
                }));
                Ok(false)
            }
            Msg::SetWeekStart(week_start) => {
                self.router.push(Route::Dashboard(DashboardQuery {
                    week_start,
                    ..self.dashboard_query(ctx)
                }));
                Ok(false)
            }
            Msg::ResetSchedule => {
                self.router.push(Route::Dashboard(DashboardQuery {
                    week: 0,
                    week_start: false,
                    ..self.dashboard_query(ctx)
                }));
                Ok(false)
            }
            Msg::SetRange(range) => {
                self.router.push(Route::Dashboard(DashboardQuery {
                    range,
                    ..self.dashboard_query(ctx)
                }));
                Ok(false)
            }
            Msg::SetSelection(selection) => {
                // Back to the first page: the filtered list is a different list.
                self.router.push(Route::Dashboard(DashboardQuery {
                    selection,
                    page: 0,
                    ..self.dashboard_query(ctx)
                }));
                Ok(false)
            }
        }
    }
}
