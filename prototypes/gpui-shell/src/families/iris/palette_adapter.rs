//! Concrete Iris Palette binding for the XDG application catalog.
//!
//! This deliberately exposes only application records. Iris settings/actions,
//! clipboard history, math and web providers remain unavailable until their
//! real services have GPUI adapters.

use std::sync::Arc;

use gpui::{App, AppContext, Entity, FocusHandle};

use crate::families::waffle::taskbar::{
    start_menu::QueryAction,
    start_menu_adapter::{StartMenuDataSource, desktop_entries::XdgDesktopEntrySource},
    start_menu_search::StartMenuSearch,
};

use super::palette::{PaletteGroup, PaletteResult, PaletteStatus};

pub struct IrisPaletteData {
    pub query: String,
    pub groups: Vec<PaletteGroup>,
    pub status: PaletteStatus,
    pub status_message: Option<String>,
}

/// Create the same Unicode/IME-capable editor used by Waffle's launcher.
pub fn search_editor(
    query: String,
    on_query_changed: QueryAction,
    focus_handle: FocusHandle,
    cx: &mut App,
) -> Entity<StartMenuSearch> {
    cx.new(|cx| {
        StartMenuSearch::new_with_shared_focus(
            query,
            "Search applications".to_owned(),
            "Search applications".to_owned(),
            on_query_changed,
            focus_handle,
            cx,
        )
    })
}

/// Convert only real XDG application records into Iris Palette groups.
pub fn snapshot(source: Option<&Arc<XdgDesktopEntrySource>>) -> IrisPaletteData {
    let Some(source) = source else {
        return IrisPaletteData {
            query: String::new(),
            groups: Vec::new(),
            status: PaletteStatus::Error,
            status_message: Some("Application search is unavailable.".into()),
        };
    };
    let Some(data) = source.snapshot() else {
        return IrisPaletteData {
            query: String::new(),
            groups: Vec::new(),
            status: PaletteStatus::Error,
            status_message: Some("Application catalog could not be read.".into()),
        };
    };
    let results = data
        .results
        .into_iter()
        .map(|entry| {
            let accessible_label = format!("{} application", entry.name);
            PaletteResult {
                id: entry.id.clone(),
                title: entry.name,
                detail: (!entry.detail.is_empty()).then_some(entry.detail),
                leading: (!entry.icon.is_empty()).then_some(entry.icon),
                trailing: Some(entry.category),
                accessible_label,
            }
        })
        .collect::<Vec<_>>();
    let empty_query = data.query.trim().is_empty();
    let no_results = results.is_empty();
    IrisPaletteData {
        query: data.query,
        groups: if no_results {
            Vec::new()
        } else {
            vec![PaletteGroup {
                id: "applications".into(),
                title: "Applications".into(),
                results,
            }]
        },
        status: if no_results && !empty_query {
            PaletteStatus::Empty
        } else {
            PaletteStatus::Ready
        },
        status_message: (no_results && !empty_query)
            .then(|| "No applications match this search.".into()),
    }
}

pub fn query_action(source: Arc<XdgDesktopEntrySource>) -> QueryAction {
    Arc::new(move |query| source.query_changed(query))
}

pub fn result_action(source: Arc<XdgDesktopEntrySource>) -> super::palette::ResultAction {
    Arc::new(move |id| {
        let _ = source.launch_app(id);
    })
}
