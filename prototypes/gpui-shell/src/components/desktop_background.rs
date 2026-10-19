//! Static ii desktop wallpaper surface for the GPUI migration.
//!
//! QML sources: `modules/background/Background.qml` and
//! `modules/background/Backdrop.qml`. This component is deliberately data
//! driven: output selection, wallpaper resolution, and the family-aware color
//! scheme are supplied by its host. It does not read global config or invent
//! palette values.

use std::{path::PathBuf, sync::Arc};

use gpui::{
    Hsla, ObjectFit, RenderImage, Role, SharedString, Window, div, img, prelude::*, px, rgba,
};

/// Wallpaper availability is resolved by the host, which owns file checks and
/// async loading. A `Ready` path may still briefly show GPUI's loading state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WallpaperStatus {
    #[default]
    Loading,
    Ready,
    Missing,
    Unsupported,
}

/// Colors are semantic roles copied from the already-resolved scheme.
/// `None` means that the resolver has not supplied that role yet.
#[derive(Clone, Copy, Debug, Default)]
pub struct DesktopBackgroundColors {
    pub background: Option<Hsla>,
    pub foreground: Option<Hsla>,
}

#[derive(Clone, Debug)]
pub struct DesktopBackgroundProps {
    /// Stable compositor output name; used by the host to select per-output
    /// wallpaper data and retained here for diagnostics/integration.
    pub output_name: String,
    pub wallpaper_path: Option<PathBuf>,
    /// A host-decoded still can be supplied to avoid repeating IO/decode work.
    pub decoded_image: Option<Arc<RenderImage>>,
    pub status: WallpaperStatus,
    pub colors: DesktopBackgroundColors,
}

impl Default for DesktopBackgroundProps {
    fn default() -> Self {
        Self {
            output_name: String::new(),
            wallpaper_path: None,
            decoded_image: None,
            status: WallpaperStatus::Loading,
            colors: DesktopBackgroundColors::default(),
        }
    }
}

pub struct DesktopBackground {
    pub props: DesktopBackgroundProps,
}

impl DesktopBackground {
    pub fn new(props: DesktopBackgroundProps) -> Self {
        Self { props }
    }

    /// Give this reusable surface a stable GPUI element ID and accessible
    /// name. The default constructor keeps the historical ii semantics.
    pub fn with_accessibility(
        props: DesktopBackgroundProps,
        element_id: impl Into<SharedString>,
        label: impl Into<SharedString>,
    ) -> DesktopBackgroundWithAccessibility {
        DesktopBackgroundWithAccessibility {
            props,
            element_id: element_id.into(),
            label: label.into(),
        }
    }
}

impl Render for DesktopBackground {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        render_desktop_background(&self.props, None)
    }
}

/// Accessibility-configured variant that leaves `DesktopBackground`'s public
/// struct-literal API intact.
pub struct DesktopBackgroundWithAccessibility {
    pub props: DesktopBackgroundProps,
    element_id: SharedString,
    label: SharedString,
}

impl Render for DesktopBackgroundWithAccessibility {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        render_desktop_background(&self.props, Some((&self.element_id, &self.label)))
    }
}

fn render_desktop_background(
    props: &DesktopBackgroundProps,
    accessibility: Option<(&SharedString, &SharedString)>,
) -> impl IntoElement {
    let background = props
        .colors
        .background
        .unwrap_or_else(|| rgba(0x00000000).into());
    let foreground = props.colors.foreground;

    let wallpaper = match (&props.decoded_image, &props.wallpaper_path, props.status) {
        (Some(image), _, WallpaperStatus::Ready) => {
            Some(img(image.clone()).object_fit(ObjectFit::Cover))
        }
        (None, Some(path), WallpaperStatus::Ready | WallpaperStatus::Loading) => {
            let loading_color = foreground;
            Some(
                img(path.clone())
                    .object_fit(ObjectFit::Cover)
                    .with_loading(move || {
                        let mut loading = div()
                            .id("desktop-background.loading")
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .role(Role::Status)
                            .aria_label("Loading wallpaper")
                            .bg(rgba(0x00000000))
                            .child("Loading wallpaper…");
                        if let Some(color) = loading_color {
                            loading = loading.text_color(color);
                        }
                        loading.into_any_element()
                    })
                    .with_fallback(move || {
                        let mut fallback = div()
                            .id("desktop-background.fallback")
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .role(Role::Status)
                            .aria_label("Wallpaper unavailable")
                            .bg(rgba(0x00000000))
                            .child("Wallpaper unavailable");
                        if let Some(color) = foreground {
                            fallback = fallback.text_color(color);
                        }
                        fallback.into_any_element()
                    }),
            )
        }
        _ => None,
    };

    let status_label = match props.status {
        WallpaperStatus::Loading if wallpaper.is_none() => Some("Loading wallpaper…"),
        WallpaperStatus::Missing => Some("Wallpaper unavailable"),
        WallpaperStatus::Unsupported => Some("Wallpaper format unsupported"),
        _ => None,
    };

    let mut root = div()
        .size_full()
        .min_w(px(1.))
        .min_h(px(1.))
        .relative()
        .overflow_hidden()
        .bg(background);

    if let Some(image) = wallpaper {
        root = root.child(image.size_full());
    }

    if let Some(label) = status_label {
        let mut status = div()
            .id("desktop-background.status")
            .absolute()
            .top(px(24.))
            .left(px(24.))
            .role(Role::Status)
            .aria_label(label)
            .child(label);
        if let Some(color) = foreground {
            status = status.text_color(color);
        }
        root = root.child(status);
    }

    if let Some((element_id, label)) = accessibility {
        div()
            .id(element_id.clone())
            .role(Role::Group)
            .aria_label(label.clone())
            .size_full()
            .child(root)
            .into_any_element()
    } else {
        root.into_any_element()
    }
}
