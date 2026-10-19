//! Adapter contract between Waffle launcher services and the Start Menu view.
//!
//! The current GPUI host has no desktop-entry or launcher-search client. This
//! module deliberately accepts service-owned snapshots and actions instead of
//! discovering, synthesizing, or launching applications itself.

#[path = "desktop_entries.rs"]
pub mod desktop_entries;

use std::sync::Arc;

use super::start_menu::{
    ItemAction, MenuAction, MenuApp, QueryAction, SearchResult, SearchState, StartMenuInput,
    StartMenuStyle,
};

#[derive(Clone, Debug, Default)]
pub struct StartMenuData {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub search_state: SearchState,
    pub selected_result: usize,
    pub pinned_apps: Vec<MenuApp>,
    pub recent_apps: Vec<MenuApp>,
    pub all_apps: Vec<MenuApp>,
    pub show_all_apps: bool,
}

/// Operations supplied by a real desktop-entry/search/launcher service.
/// Implementations should report only records actually available to them.
pub trait StartMenuDataSource: Send + Sync {
    fn snapshot(&self) -> Option<StartMenuData>;
    fn query_changed(&self, query: String);
    fn select_result(&self, id: String);
    fn launch_app(&self, id: String) -> Result<(), String>;
    fn show_all_apps(&self);
    fn back(&self);
    fn close(&self);
}

/// Build the presentation input from the connected adapter. With no adapter,
/// produce a truthful unavailable state with empty data and inert callbacks.
pub fn input(
    style: StartMenuStyle,
    source: Option<Arc<dyn StartMenuDataSource>>,
) -> StartMenuInput {
    let data = source.as_ref().and_then(|source| source.snapshot());
    let unavailable = data.is_none();
    let fallback = StartMenuData {
        search_state: SearchState::Failed,
        ..StartMenuData::default()
    };
    let data = data.unwrap_or(fallback);

    let on_query_changed: QueryAction = source
        .as_ref()
        .map(|source| {
            let source = Arc::clone(source);
            Arc::new(move |query| source.query_changed(query)) as QueryAction
        })
        .unwrap_or_else(|| Arc::new(|_| {}));
    let on_select_result: ItemAction = source
        .as_ref()
        .map(|source| {
            let source = Arc::clone(source);
            Arc::new(move |id| source.select_result(id)) as ItemAction
        })
        .unwrap_or_else(|| Arc::new(|_| {}));
    let on_launch_app: ItemAction = source
        .as_ref()
        .map(|source| {
            let source = Arc::clone(source);
            Arc::new(move |id: String| {
                if let Err(error) = source.launch_app(id.clone()) {
                    eprintln!("GPUI app launcher: could not launch '{id}': {error}");
                }
            }) as ItemAction
        })
        .unwrap_or_else(|| Arc::new(|_| {}));
    let on_all_apps: MenuAction = source
        .as_ref()
        .map(|source| {
            let source = Arc::clone(source);
            Arc::new(move || source.show_all_apps()) as MenuAction
        })
        .unwrap_or_else(|| Arc::new(|| {}));
    let on_back: MenuAction = source
        .as_ref()
        .map(|source| {
            let source = Arc::clone(source);
            Arc::new(move || source.back()) as MenuAction
        })
        .unwrap_or_else(|| Arc::new(|| {}));
    let on_close: MenuAction = source
        .map(|source| Arc::new(move || source.close()) as MenuAction)
        .unwrap_or_else(|| Arc::new(|| {}));

    StartMenuInput {
        style,
        query: data.query,
        results: data.results,
        search_state: if unavailable {
            SearchState::Failed
        } else {
            data.search_state
        },
        selected_result: data.selected_result,
        pinned_apps: data.pinned_apps,
        recent_apps: data.recent_apps,
        all_apps: data.all_apps,
        show_all_apps: data.show_all_apps,
        on_query_changed,
        on_select_result,
        on_launch_app,
        on_all_apps,
        on_back,
        on_close,
    }
}
