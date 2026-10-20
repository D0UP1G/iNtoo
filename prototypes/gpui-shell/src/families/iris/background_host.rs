//! Explicit, opt-in GPUI preview of Iris's configured desktop wallpaper.
//!
//! This is a wallpaper-host seam, not a production Iris shell route. The view
//! renders the typed config snapshot for one explicitly selected GPUI display
//! and visibly reports the Quickshell runtime providers that are not connected.

use std::path::PathBuf;

use gpui::{Hsla, ObjectFit, Role, Window, div, img, prelude::*, px, rgba};

use super::background_state::{
    DesktopBackgroundMode, MotionDecision, OutputBackgroundPolicy, PresentationRuntimeInput,
    WallpaperMediaKind, WallpaperPresentationDecision,
};

#[derive(Clone, Debug)]
pub struct IrisBackgroundPreviewInput {
    pub policy: OutputBackgroundPolicy,
    pub background: Hsla,
    pub foreground: Hsla,
}

pub struct IrisBackgroundPreviewView {
    pub input: IrisBackgroundPreviewInput,
}

impl IrisBackgroundPreviewView {
    pub fn new(input: IrisBackgroundPreviewInput) -> Self {
        Self { input }
    }
}

impl Render for IrisBackgroundPreviewView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let input = &self.input;
        let mut background = input.background;
        background.a = 1.0;
        let output = if input.policy.output_name.is_empty() {
            "output identity unavailable".to_owned()
        } else {
            format!("output {}", input.policy.output_name)
        };
        let id_output = if input.policy.output_name.is_empty() {
            "unresolved"
        } else {
            input.policy.output_name.as_str()
        };

        let mut root = div()
            .id(format!("iris.background.preview.{id_output}"))
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(background);

        let gpui_may_show_config_snapshot = matches!(
            input.policy.presentation,
            WallpaperPresentationDecision::RenderInternally
                | WallpaperPresentationDecision::Awaiting(_)
        );
        if gpui_may_show_config_snapshot
            && input.policy.media == WallpaperMediaKind::Still
            && !input.policy.wallpaper.path.is_empty()
        {
            root = root.child(
                img(PathBuf::from(&input.policy.wallpaper.path))
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }

        let mode_text = match input.policy.mode {
            DesktopBackgroundMode::WidgetCanvas => {
                "Mode: desktop widget canvas (widgets are not hosted in this preview)"
            }
            DesktopBackgroundMode::WallpaperOnly => "Mode: wallpaper-only desktop",
        };
        let media_text = match (input.policy.presentation, input.policy.media) {
            (WallpaperPresentationDecision::ExternalAwwwOwnsWallpaper, _) => {
                "Awww is the reported wallpaper owner; GPUI image is withheld"
            }
            (_, WallpaperMediaKind::Still) => "Configured still image snapshot",
            (_, WallpaperMediaKind::Gif) => "GIF playback is not hosted; no frame was guessed",
            (_, WallpaperMediaKind::Video) => "Video playback is not hosted; no frame was guessed",
            (_, WallpaperMediaKind::Empty) => {
                "No wallpaper path is configured; showing opaque fallback"
            }
            (_, WallpaperMediaKind::Unsupported) => {
                "This path is unsupported by the current static GPUI image host"
            }
        };
        let presentation_text = match input.policy.presentation {
            WallpaperPresentationDecision::RenderInternally => {
                "Supplied runtime state selects internal wallpaper rendering"
            }
            WallpaperPresentationDecision::ExternalAwwwOwnsWallpaper => {
                "Awww owns the wallpaper; the GPUI image path is withheld"
            }
            WallpaperPresentationDecision::Awaiting(reason) => match reason {
                PresentationRuntimeInput::OutputIdentity => {
                    "Output name is unresolved; no per-monitor path was selected"
                }
                PresentationRuntimeInput::InternalPreviewState => {
                    "Preview state is not connected; this view shows the configured path snapshot"
                }
                PresentationRuntimeInput::InternalPreviewTarget => {
                    "Preview target is not connected; configured path snapshot only"
                }
                PresentationRuntimeInput::InternalPreviewPath => {
                    "Preview path is not connected; configured path snapshot only"
                }
                PresentationRuntimeInput::AwwwOwnership => {
                    "Awww eligibility is not connected; opt-in GPUI layer owns this preview"
                }
            },
        };
        let motion_text = match input.policy.motion {
            MotionDecision::Allowed => {
                "Movement policy inputs allow motion; this view still renders only a static image"
                    .to_owned()
            }
            MotionDecision::Paused(reason) => format!("Motion paused: {reason:?}"),
            MotionDecision::Awaiting(input) => format!("Motion provider not connected: {input:?}"),
        };
        let title = format!("Iris configured wallpaper preview · {output}");
        let foreground = input.foreground;

        root.child(
            div()
                .id("iris.background.preview.status")
                .absolute()
                .top(px(20.))
                .left(px(20.))
                .max_w(px(680.))
                .rounded(px(12.))
                .bg(rgba(0x111318ee))
                .text_color(foreground)
                .px_4()
                .py_3()
                .role(Role::Status)
                .aria_label(title.clone())
                .flex()
                .flex_col()
                .gap_1()
                .child(title)
                .child(mode_text)
                .child(media_text)
                .child(presentation_text)
                .child(motion_text)
                .child("Opt-in config snapshot only; QML gate, preview, Awww and motion parity are unverified"),
        )
    }
}
