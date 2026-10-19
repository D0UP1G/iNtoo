//! Data-driven full-output ii backdrop presentation.
//!
//! Per-output wallpaper resolution, async decoding, and config remain owned by
//! the host. This view renders only a supplied still image and supported
//! overlays; it never reads the filesystem or starts a media decoder.

use std::{path::PathBuf, sync::Arc};

use gpui::{
    AnyElement, Hsla, IntoElement, LinearColorStop, ObjectFit, RenderImage, Role, SharedString,
    Window, div, hsla, img, linear_gradient, prelude::*, px, rgba,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackdropStatus {
    #[default]
    Loading,
    Ready,
    Missing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WallpaperKind {
    #[default]
    Still,
    Video,
    AnimatedImage,
    Unsupported,
}

#[derive(Clone, Copy, Debug)]
pub struct Vignette {
    /// Black overlay strength, clamped to 0..=1.
    pub intensity: f32,
    /// Point in the gradient where darkening begins, clamped to 0..=1.
    pub radius: f32,
}

#[derive(Clone, Debug)]
pub struct DesktopBackdropInput {
    pub output_name: String,
    pub wallpaper_path: Option<PathBuf>,
    /// An image decoded by the host for this output. Only used for stills.
    pub decoded_image: Option<Arc<RenderImage>>,
    pub status: BackdropStatus,
    pub wallpaper_kind: WallpaperKind,
    pub use_aurora_style: bool,
    pub aurora_overlay_opacity: f32,
    pub enable_animation: bool,
    /// QML backdrop dim percentage, 0..=100.
    pub dim_percent: f32,
    /// Values are retained from host config for parity; GPUI currently has no
    /// image saturation/contrast filter, so non-neutral requests are reported.
    pub saturation: f32,
    pub contrast: f32,
    /// QML blur radius, retained for parity and reported as unsupported.
    pub blur_radius: f32,
    pub vignette: Option<Vignette>,
    /// Reduced effects suppress the vignette and unsupported filter request.
    pub reduced_effects: bool,
    pub background: Hsla,
    pub foreground: Hsla,
}

impl Default for DesktopBackdropInput {
    fn default() -> Self {
        Self {
            output_name: String::new(),
            wallpaper_path: None,
            decoded_image: None,
            status: BackdropStatus::Loading,
            wallpaper_kind: WallpaperKind::Still,
            use_aurora_style: false,
            aurora_overlay_opacity: 0.38,
            enable_animation: false,
            dim_percent: 35.,
            saturation: 0.,
            contrast: 0.,
            blur_radius: 0.,
            vignette: None,
            reduced_effects: false,
            background: rgba(0x00000000).into(),
            foreground: rgba(0xffffffff).into(),
        }
    }
}

pub struct DesktopBackdrop {
    pub input: DesktopBackdropInput,
}

impl DesktopBackdrop {
    pub fn new(input: DesktopBackdropInput) -> Self {
        Self { input }
    }
}

impl Render for DesktopBackdrop {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        render_desktop_backdrop(&self.input)
    }
}

pub fn render_desktop_backdrop(input: &DesktopBackdropInput) -> AnyElement {
    let mut root = div()
        .id(SharedString::from(format!(
            "ii.desktop_backdrop.{}",
            if input.output_name.is_empty() {
                "default"
            } else {
                input.output_name.as_str()
            }
        )))
        .size_full()
        .min_w(px(1.))
        .min_h(px(1.))
        .relative()
        .overflow_hidden()
        .bg(input.background)
        .role(Role::Group)
        .aria_label(if input.output_name.is_empty() {
            "Desktop wallpaper".to_owned()
        } else {
            format!("Desktop wallpaper for output {}", input.output_name)
        });

    if input.wallpaper_kind == WallpaperKind::Still {
        if input.status != BackdropStatus::Missing
            && let Some(image) = &input.decoded_image
        {
            root = root.child(img(image.clone()).size_full().object_fit(ObjectFit::Cover));
        } else if let Some(path) = &input.wallpaper_path
            && matches!(
                input.status,
                BackdropStatus::Loading | BackdropStatus::Ready
            )
        {
            let foreground = input.foreground;
            root = root.child(
                img(path.clone())
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .with_loading(move || {
                        message_card(
                            "ii.desktop_backdrop.image_loading",
                            "Loading wallpaper…",
                            foreground,
                        )
                    })
                    .with_fallback(move || {
                        message_card(
                            "ii.desktop_backdrop.image_fallback",
                            "Wallpaper unavailable",
                            foreground,
                        )
                    }),
            );
        }
    }

    let status_message = match input.wallpaper_kind {
        WallpaperKind::Video => Some("Video wallpaper preview is unsupported in GPUI"),
        WallpaperKind::AnimatedImage => Some("Animated wallpaper playback is unsupported in GPUI"),
        WallpaperKind::Unsupported => Some("This wallpaper format is unsupported in GPUI"),
        WallpaperKind::Still if input.status == BackdropStatus::Missing => {
            Some("Wallpaper unavailable")
        }
        WallpaperKind::Still
            if input.status == BackdropStatus::Loading
                && input.wallpaper_path.is_none()
                && input.decoded_image.is_none() =>
        {
            Some("Loading wallpaper…")
        }
        WallpaperKind::Still
            if input.status == BackdropStatus::Ready
                && input.wallpaper_path.is_none()
                && input.decoded_image.is_none() =>
        {
            Some("Wallpaper source unavailable")
        }
        _ => None,
    };

    if input.wallpaper_kind == WallpaperKind::Still {
        if !input.use_aurora_style {
            let dim_alpha = (input.dim_percent.clamp(0., 100.) / 100.).clamp(0., 1.);
            if dim_alpha > 0. {
                root = root.child(
                    div()
                        .id("ii.desktop_backdrop.dim")
                        .absolute()
                        .inset_0()
                        .bg(hsla(0., 0., 0., dim_alpha)),
                );
            }
        }
        if !input.reduced_effects {
            if let Some(vignette) = input.vignette {
                let intensity = vignette.intensity.clamp(0., 1.);
                if intensity > 0. {
                    let radius = vignette.radius.clamp(0., 1.);
                    root = root.child(
                        div()
                            .id("ii.desktop_backdrop.vignette")
                            .absolute()
                            .inset_0()
                            .bg(linear_gradient(
                                180.,
                                LinearColorStop {
                                    color: hsla(0., 0., 0., 0.),
                                    percentage: radius,
                                },
                                LinearColorStop {
                                    color: hsla(0., 0., 0., intensity),
                                    percentage: 1.,
                                },
                            )),
                    );
                }
            }
        }
    }

    if let Some(message) = status_message {
        root = root.child(
            div()
                .id("ii.desktop_backdrop.wallpaper_state")
                .absolute()
                .top(px(24.))
                .left(px(24.))
                .role(Role::Status)
                .aria_label(message)
                .rounded(px(8.))
                .bg(hsla(0., 0., 0.7, 0.72))
                .text_color(input.foreground)
                .px_3()
                .py_2()
                .child(message),
        );
    }

    let mut unsupported = Vec::new();
    if !input.reduced_effects {
        if input.blur_radius > 0. {
            unsupported.push("blur");
        }
        if input.saturation.abs() > f32::EPSILON {
            unsupported.push("saturation");
        }
        if input.contrast.abs() > f32::EPSILON {
            unsupported.push("contrast");
        }
        if input.use_aurora_style {
            unsupported.push("Aurora adaptive overlay");
            if input.aurora_overlay_opacity > 0. {
                unsupported.push("Aurora overlay opacity");
            }
        }
    }
    if !unsupported.is_empty() {
        let detail = format!("Unsupported effects: {}", unsupported.join(", "));
        root = root.child(
            div()
                .id("ii.desktop_backdrop.unsupported_effects")
                .absolute()
                .right(px(24.))
                .top(px(24.))
                .role(Role::Status)
                .aria_label(detail.clone())
                .rounded(px(8.))
                .bg(hsla(0., 0., 0.7, 0.72))
                .text_color(input.foreground)
                .px_3()
                .py_2()
                .child(detail),
        );
    } else if input.reduced_effects {
        root = root.child(
            div()
                .id("ii.desktop_backdrop.reduced_effects")
                .absolute()
                .right(px(24.))
                .top(px(24.))
                .role(Role::Status)
                .aria_label("Reduced visual effects enabled")
                .rounded(px(8.))
                .bg(hsla(0., 0., 0.7, 0.72))
                .text_color(input.foreground)
                .px_3()
                .py_2()
                .child("Reduced visual effects"),
        );
    }

    root.into_any_element()
}

fn message_card(id: &'static str, message: &'static str, foreground: Hsla) -> AnyElement {
    div()
        .id(id)
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .role(Role::Status)
        .aria_label(message)
        .text_color(foreground)
        .child(message)
        .into_any_element()
}
