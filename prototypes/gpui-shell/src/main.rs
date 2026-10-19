mod audio_provider;
mod bar;
mod config;
pub mod components {
    pub mod desktop_backdrop;
    pub mod desktop_background;
    pub mod material;
    pub mod top_bar;
    pub mod vertical_bar;
}
mod families {
    pub mod iris {
        pub mod chassis;
        pub mod dock;
        pub mod orbit;
        pub mod palette;
        pub mod palette_adapter;
        pub mod runtime;
    }
    pub mod ii {
        pub mod context_weather;
        pub mod context_weather_state;
        pub mod controls_card;
        pub mod crypto_state;
        pub mod crypto_widget;
        pub mod dock;
        pub mod dock_preview;
        pub mod dock_preview_cache;
        pub mod dock_preview_host;
        pub mod dock_surface;
        pub mod glance_clock;
        pub mod left_sidebar_tabs;
        pub mod media_player;
        pub mod notification_panel;
        pub mod overview;
        pub mod overview_overlay;
        pub mod panel_surface;
        pub mod quick_launch;
        pub mod quick_note;
        pub mod quick_note_store;
        pub mod quick_sliders;
        pub mod quick_toggles;
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
mod niri;
mod notification_service;
mod resource_provider;
mod surface_manager;
pub mod theme;
mod theme_resolver;
mod vertical_provider;

fn main() {
    gpui_platform::application().run(|cx| {
        surface_manager::start(cx);
    });
}
