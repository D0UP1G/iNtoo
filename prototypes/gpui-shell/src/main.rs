mod audio_provider;
mod bar;
mod config;
pub mod components {
    pub mod desktop_backdrop;
    pub mod desktop_background;
    pub mod horizontal_battery;
    pub mod horizontal_media;
    pub mod material;
    pub mod media_controls_popup;
    pub mod media_controls_surface;
    pub mod top_bar;
    pub mod vertical_bar;
}
mod families {
    pub mod iris {
        pub mod background_host;
        pub mod background_state;
        pub mod chassis;
        pub mod dock;
        pub mod orbit;
        pub mod palette;
        pub mod palette_adapter;
        pub mod runtime;
    }
    pub mod ii {
        pub mod cheatsheet;
        pub mod cheatsheet_elements;
        pub mod cheatsheet_host;
        pub mod clipboard;
        pub mod clipboard_host;
        pub mod context_weather;
        pub mod context_weather_state;
        pub mod control_panel;
        pub mod controls_card;
        pub mod crypto_state;
        pub mod crypto_widget;
        pub mod dock;
        pub mod dock_preview;
        pub mod dock_preview_cache;
        pub mod dock_preview_host;
        pub mod dock_surface;
        pub mod glance_clock;
        pub mod keybind_provider;
        pub mod left_sidebar_tabs;
        pub mod lock;
        pub mod media_player;
        pub mod notification_panel;
        pub mod notification_popup;
        pub mod osd;
        pub mod overview;
        pub mod overview_overlay;
        pub mod panel_surface;
        pub mod quick_launch;
        pub mod quick_note;
        pub mod quick_note_store;
        pub mod quick_sliders;
        pub mod quick_toggles;
        pub mod session_screen;
        pub mod session_screen_host;
        pub mod session_screen_state;
        pub mod sidebars;
        pub mod status_rings;
        pub mod system_sidebar_header;
        pub mod volume_mixer;
        pub mod wallpaper_gallery;
        pub mod wallpaper_gallery_state;
        pub mod week_strip;
        pub mod week_strip_state;
        pub mod world_clock;
        pub mod world_clock_state;
    }
    pub mod waffle {
        pub mod action_center;
        pub mod action_center_overlay;
        pub mod clipboard;
        pub mod clipboard_overlay;
        pub mod clipboard_provider;
        pub mod controller;
        pub mod notification_center;
        pub mod notification_overlay;
        pub mod runtime;
        pub mod task_view;
        pub mod task_view_overlay;
        pub mod taskbar;
    }
}
mod family_host;
mod mvp_settings;
mod niri;
mod notification_service;
mod resource_provider;
mod surface_manager;
pub mod theme;
mod theme_resolver;
mod vertical_provider;

fn main() {
    let preview_display_index = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|args| args[0] == "--session-screen-preview-display")
        .and_then(|args| args[1].parse::<usize>().ok());
    let clipboard_preview_display_index = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|args| args[0] == "--ii-clipboard-preview-display")
        .and_then(|args| args[1].parse::<usize>().ok());
    let cheatsheet_preview_display_index = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|args| args[0] == "--ii-cheatsheet-preview-display")
        .and_then(|args| args[1].parse::<usize>().ok());
    let iris_background_preview_display_index = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|args| args[0] == "--iris-background-preview-display")
        .and_then(|args| args[1].parse::<usize>().ok());
    gpui_platform::application().run(move |cx| {
        surface_manager::start_with_iris_background_preview(
            cx,
            iris_background_preview_display_index,
        );
        if let Some(index) = preview_display_index {
            // The index is explicitly provided for visual testing; it does
            // not infer the compositor-focused output. Output ordering is an
            // unverified preview-only fallback until a focused-output provider
            // is integrated.
            let displays = cx.displays();
            if let Some(display) = displays.get(index) {
                let _ = surface_manager::toggle_ii_session_screen(display.id());
            } else {
                eprintln!(
                    "Session Screen preview: display index {index} is unavailable ({} output(s))",
                    displays.len()
                );
            }
        }
        if let Some(index) = clipboard_preview_display_index {
            // Opt-in preview only. GPUI does not yet own the production
            // clipboard shortcut/launcher action producer.
            let displays = cx.displays();
            if let Some(display) = displays.get(index) {
                let _ = surface_manager::open_ii_clipboard_preview(display.id());
            } else {
                eprintln!(
                    "ii Clipboard preview: display index {index} is unavailable ({} output(s))",
                    displays.len()
                );
            }
        }
        if let Some(index) = cheatsheet_preview_display_index {
            // The supplied GPUI display index maps directly to this DisplayId;
            // the test route never follows compositor focus or screen order.
            let displays = cx.displays();
            if let Some(display) = displays.get(index) {
                if !surface_manager::open_ii_cheatsheet_preview(display.id()) {
                    eprintln!(
                        "ii Cheatsheet preview: route was not ready for display index {index}"
                    );
                }
            } else {
                eprintln!(
                    "ii Cheatsheet preview: display index {index} is unavailable ({} output(s))",
                    displays.len()
                );
            }
        }
    });
}
