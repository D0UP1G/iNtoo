//! Config driven tab strip for the II feature sidebar.
//!
//! The strip only selects stable tab IDs. The matching content views and their
//! providers remain the responsibility of the host adapter.

use std::{collections::HashMap, sync::Arc};

use gpui::{
    AnyElement, FocusHandle, FontWeight, IntoElement, KeyDownEvent, Role, ScrollHandle,
    SharedString, div, prelude::*, px,
};

use crate::{config::SidebarLeftTab, families::ii::sidebars::SidebarStyle};

pub type SelectTab = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Clone)]
pub struct LeftSidebarTabsInput {
    /// Configuration entries with their QML stable IDs and enablement state.
    pub tabs: Vec<SidebarLeftTab>,
    /// Resolved QML order. Missing enabled IDs are appended in input order.
    pub order: Vec<String>,
    /// Current selected stable ID, if the host has one.
    pub selected_tab_id: Option<String>,
    pub style: SidebarStyle,
    pub focus_handles: HashMap<String, FocusHandle>,
    pub scroll: ScrollHandle,
    pub on_select: SelectTab,
}

/// Render the enabled tabs as an accessible, horizontally scrollable tablist.
pub fn render_left_sidebar_tabs(input: LeftSidebarTabsInput) -> AnyElement {
    let mut ordered_ids = input.order;
    for tab in &input.tabs {
        if tab.enabled && !ordered_ids.iter().any(|id| id == &tab.id) {
            ordered_ids.push(tab.id.clone());
        }
    }
    let tabs = ordered_ids
        .into_iter()
        .filter_map(|id| {
            let tab = input.tabs.iter().find(|tab| tab.enabled && tab.id == id)?;
            let (label, icon) = tab_presentation(&tab.id)?;
            Some((tab.id.clone(), label, icon))
        })
        .collect::<Vec<_>>();
    let compact = tabs.len() >= 4;
    let selected_id = input.selected_tab_id;
    let style = input.style;
    let select = input.on_select;
    let scroll = input.scroll;
    let focus_handles = input.focus_handles;
    let selected_index = selected_id
        .as_deref()
        .and_then(|selected| tabs.iter().position(|(id, _, _)| id == selected))
        .unwrap_or(0);
    let tab_count = tabs.len();

    let tab_ids = tabs.iter().map(|(id, _, _)| id.clone()).collect::<Vec<_>>();
    let buttons = tabs
        .into_iter()
        .enumerate()
        .map(|(index, (id, label, icon))| {
            let selected = selected_id.as_deref() == Some(id.as_str());
            let tab_stop = selected || (selected_id.is_none() && index == selected_index);
            let click_select = select.clone();
            let key_select = select.clone();
            let previous_index = (tab_count > 1).then_some((index + tab_count - 1) % tab_count);
            let next_index = (tab_count > 1).then_some((index + 1) % tab_count);
            let first_index = (index != 0).then_some(0);
            let last_index = (index + 1 != tab_count).then_some(tab_count - 1);
            let keyboard_tab_ids = tab_ids.clone();
            let current_focus_handle = focus_handles.get(&id).cloned();
            let key_focus_handles = focus_handles.clone();
            let current_scroll = scroll.clone();
            let accessible_name = SharedString::from(label);
            div()
                .id(SharedString::from(format!("ii.sidebar.left.tab.{id}")))
                .flex_shrink_0()
                .h(px(32.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.0))
                .rounded(px(16.0))
                .bg(if selected {
                    style.selected
                } else {
                    style.surface
                })
                .text_color(style.foreground)
                .role(Role::Tab)
                .aria_label(accessible_name)
                .aria_selected(selected)
                .focusable()
                .tab_stop(tab_stop)
                .when_some(current_focus_handle.as_ref(), |tab, handle| {
                    tab.track_focus(handle)
                })
                .cursor_pointer()
                .hover({
                    let hover = style.hover;
                    move |s| s.bg(hover)
                })
                .focus_visible({
                    let border = style.border;
                    move |s| s.border_1().border_color(border)
                })
                .on_click({
                    let id = id.clone();
                    let focus_handle = current_focus_handle.clone();
                    move |_, window, cx| {
                        click_select(id.clone());
                        if let Some(handle) = focus_handle.as_ref() {
                            window.focus(handle, cx);
                        }
                    }
                })
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.is_held {
                        return;
                    }
                    let target = match event.keystroke.key.as_str() {
                        "enter" | "space" => Some(index),
                        "left" => previous_index,
                        "right" => next_index,
                        "home" => first_index,
                        "end" => last_index,
                        _ => None,
                    };
                    if let Some(target) = target {
                        if let Some(target_id) = keyboard_tab_ids.get(target) {
                            key_select(target_id.clone());
                            if let Some(handle) = key_focus_handles.get(target_id) {
                                window.focus(handle, cx);
                            }
                            current_scroll.scroll_to_item(target);
                            cx.stop_propagation();
                        }
                    }
                })
                .child(div().font_family("Material Symbols Rounded").child(icon))
                .when(!compact || selected, |tab| {
                    tab.child(
                        div()
                            .text_xs()
                            .font_weight(if selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(label),
                    )
                })
                .into_any_element()
        });

    div()
        .id("ii.sidebar.left.tabs")
        .w_full()
        .min_w_0()
        .h(px(40.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .px(px(4.0))
        .overflow_x_scroll()
        .track_scroll(&scroll)
        .role(Role::TabList)
        .aria_label("Feature sidebar tabs")
        .children(buttons)
        .into_any_element()
}

fn tab_presentation(id: &str) -> Option<(&'static str, &'static str)> {
    Some(match id {
        "widgets" => ("Widgets", "widgets"),
        "ai" => ("Intelligence", "neurology"),
        "translator" => ("Translator", "translate"),
        "anime" => ("Anime", "bookmark_heart"),
        "animeSchedule" => ("Schedule", "calendar_month"),
        "wallhaven" => ("Wallpapers", "collections"),
        "news" => ("News", "newspaper"),
        "ytmusic" => ("YT Music", "library_music"),
        "tools" => ("Tools", "build"),
        "software" => ("Software", "store"),
        _ => return None,
    })
}
