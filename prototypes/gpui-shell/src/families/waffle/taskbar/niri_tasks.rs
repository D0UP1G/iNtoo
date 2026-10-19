//! Waffle task records derived from the live Niri snapshot and existing pins.

use std::collections::HashMap;

use crate::niri;

use super::{TaskEntry, start_menu_adapter::desktop_entries::XdgDesktopEntrySource};

#[derive(Clone, Debug, Default)]
pub struct TaskSnapshot {
    pub tasks: Vec<TaskEntry>,
    /// Niri window ids are in their reported tile position order when available.
    pub windows_by_app: HashMap<String, Vec<u64>>,
}

pub fn snapshot(
    output_name: Option<&str>,
    pinned_ids: &[String],
    catalog: Option<&XdgDesktopEntrySource>,
) -> TaskSnapshot {
    from_snapshot(niri::snapshot(output_name), pinned_ids, catalog)
}

pub fn from_snapshot(
    niri: niri::Snapshot,
    pinned_ids: &[String],
    catalog: Option<&XdgDesktopEntrySource>,
) -> TaskSnapshot {
    from_snapshot_with_icon_size(niri, pinned_ids, catalog, 26)
}

pub fn from_snapshot_with_icon_size(
    niri: niri::Snapshot,
    pinned_ids: &[String],
    catalog: Option<&XdgDesktopEntrySource>,
    icon_size: u32,
) -> TaskSnapshot {
    let mut ordered_ids = Vec::<String>::new();
    let mut windows_by_app = HashMap::<String, Vec<niri::Window>>::new();

    for window in niri.windows {
        let Some(app_id) = window.app_id.as_ref().filter(|id| !id.trim().is_empty()) else {
            continue;
        };
        let key = app_id.to_lowercase();
        if !windows_by_app.contains_key(&key) {
            ordered_ids.push(app_id.clone());
            windows_by_app.insert(key.clone(), Vec::new());
        }
        windows_by_app
            .get_mut(&key)
            .expect("inserted above")
            .push(window);
    }

    let mut focused_by_app = HashMap::<String, Option<usize>>::new();
    let windows_by_app = windows_by_app
        .into_iter()
        .map(|(app_id, mut windows)| {
            // QML stably sorts each app's layout-sorted windows by column for
            // its focused indicator. Row and id preserve the source tie order.
            windows.sort_by_key(|window| {
                let [column, row] = window.pos_in_scrolling_layout.unwrap_or([u64::MAX; 2]);
                (column, row, window.id)
            });
            focused_by_app.insert(
                app_id.clone(),
                windows.iter().position(|window| window.focused),
            );
            (
                app_id,
                windows
                    .into_iter()
                    .map(|window| window.id)
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut tasks = Vec::new();
    for pinned in pinned_ids {
        let Some(id) = normalize_app_id(pinned) else {
            continue;
        };
        let running = windows_by_app
            .get(&id.to_lowercase())
            .is_some_and(|windows| !windows.is_empty());
        // TaskbarApps.qml omits unresolved pins unless a live window exists.
        if !running && catalog.and_then(|source| source.resolve_app(&id)).is_none() {
            continue;
        }
        if !tasks
            .iter()
            .any(|task: &TaskEntry| task.app_id.eq_ignore_ascii_case(&id))
        {
            tasks.push(make_entry(
                &id,
                true,
                &windows_by_app,
                &focused_by_app,
                catalog,
                icon_size,
            ));
        }
    }
    let mut unpinned_ids = ordered_ids
        .into_iter()
        .filter(|id| {
            !pinned_ids
                .iter()
                .any(|pinned| pinned.eq_ignore_ascii_case(id))
        })
        .collect::<Vec<_>>();
    // Keep unpinned running tasks stable when Niri changes layout/focus order.
    unpinned_ids.sort_by_cached_key(|id| id.to_lowercase());
    for id in unpinned_ids {
        if !tasks
            .iter()
            .any(|task| task.app_id.eq_ignore_ascii_case(&id))
        {
            tasks.push(make_entry(
                &id,
                false,
                &windows_by_app,
                &focused_by_app,
                catalog,
                icon_size,
            ));
        }
    }

    TaskSnapshot {
        tasks,
        windows_by_app,
    }
}

fn make_entry(
    app_id: &str,
    pinned: bool,
    windows_by_app: &HashMap<String, Vec<u64>>,
    focused_by_app: &HashMap<String, Option<usize>>,
    catalog: Option<&XdgDesktopEntrySource>,
    icon_size: u32,
) -> TaskEntry {
    let key = app_id.to_lowercase();
    let windows = windows_by_app.get(&key);
    let (label, _) = catalog
        .and_then(|catalog| catalog.resolve_app(app_id))
        .unwrap_or_else(|| (app_id.to_owned(), None));
    let icon_path =
        catalog.and_then(|catalog| catalog.resolve_icon_path_for_size(app_id, icon_size));
    TaskEntry {
        app_id: app_id.to_owned(),
        label,
        icon_path,
        pinned,
        active: focused_by_app.get(&key).is_some_and(Option::is_some),
        window_count: windows.map_or(0, Vec::len),
        focused_window_index: focused_by_app.get(&key).copied().flatten(),
        // The prototype has no minimized-window provider.
        minimized_count: 0,
    }
}

fn normalize_app_id(id: &str) -> Option<String> {
    let id = id.trim();
    (!id.is_empty()).then(|| id.to_owned())
}
