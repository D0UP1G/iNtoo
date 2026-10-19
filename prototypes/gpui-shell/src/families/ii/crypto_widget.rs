//! Coin price rows for the ii Widgets sidebar.
//!
//! This component only presents provider supplied prices, 24-hour changes and
//! historical samples. It never invents fallback market data.

use std::sync::Arc;

use gpui::{
    AnyElement, FontWeight, IntoElement, KeyDownEvent, PathBuilder, Role, SharedString, canvas,
    div, point, prelude::*, px,
};

use super::{
    crypto_state::{CryptoCoin, CryptoState, CryptoStatus},
    sidebars::SidebarStyle,
};

pub type CryptoRefreshAction = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct CryptoWidgetInput {
    pub state: CryptoState,
    pub style: SidebarStyle,
    pub on_refresh: CryptoRefreshAction,
    pub refresh_enabled: bool,
}

impl std::fmt::Debug for CryptoWidgetInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CryptoWidgetInput")
            .field("state", &self.state)
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

/// Render the configured coin order, showing only real provider values.
pub fn render_crypto_widget(input: CryptoWidgetInput) -> AnyElement {
    let style = input.style;
    let refresh = input.on_refresh.clone();
    let refresh_enabled = input.refresh_enabled;
    let loading = matches!(&input.state.status, CryptoStatus::Loading);
    let pending_ids = input.state.pending_ids.clone();
    let rows = input.state.coins.iter().cloned().map(|coin| {
        let pending = pending_ids.iter().any(|id| id == &coin.id);
        render_coin(coin, &style, pending)
    });
    let status = match &input.state.status {
        CryptoStatus::Loading => Some((true, "Loading cryptocurrency prices…".to_owned())),
        CryptoStatus::Unavailable(message) => Some((false, message.clone())),
        CryptoStatus::Ready if input.state.coins.is_empty() => {
            Some((false, "No coins configured".to_owned()))
        }
        CryptoStatus::Ready => None,
    };
    let stale = input.state.stale;
    let updated_at = input.state.updated_at;

    let mut card = div()
        .id("ii.crypto-widget.card")
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(6.))
        .px(px(10.))
        .py(px(8.))
        .role(Role::Group)
        .aria_label("Cryptocurrency prices")
        .text_color(style.foreground)
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .font_family("Material Symbols Rounded")
                        .text_size(px(16.))
                        .text_color(style.accent)
                        .child("currency_bitcoin"),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(style.foreground)
                        .child("Crypto"),
                )
                .child(div().flex_1())
                .child(refresh_button(
                    refresh,
                    loading,
                    refresh_enabled,
                    style.clone(),
                )),
        );

    if !input.state.coins.is_empty() {
        card = card.children(rows);
    }
    if stale {
        let age_label = updated_at.map_or_else(
            || "Showing cached prices; refresh is due".to_owned(),
            |timestamp| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_secs())
                    .unwrap_or(timestamp);
                let minutes = now.saturating_sub(timestamp) / 60;
                format!("Showing cached prices · updated {minutes} min ago")
            },
        );
        card = card.child(
            div()
                .id("ii.crypto-widget.stale")
                .role(Role::Status)
                .aria_label(SharedString::from(age_label.clone()))
                .text_xs()
                .text_color(style.muted)
                .child(age_label),
        );
    }
    if let Some((is_loading, message)) = status {
        card = card.child(
            div()
                .id(if is_loading {
                    "ii.crypto-widget.status.loading"
                } else {
                    "ii.crypto-widget.status.message"
                })
                .role(Role::Status)
                .aria_label(SharedString::from(message.clone()))
                .text_xs()
                .text_color(style.muted)
                .child(message),
        );
    }

    card.into_any_element()
}

fn refresh_button(
    on_refresh: CryptoRefreshAction,
    loading: bool,
    refresh_enabled: bool,
    style: SidebarStyle,
) -> impl IntoElement {
    let click_refresh = on_refresh.clone();
    let key_refresh = on_refresh;
    let enabled = refresh_enabled && !loading;
    div()
        .id("ii.crypto-widget.refresh")
        .size(px(24.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(style.card_radius.min(12.)))
        .text_color(if enabled {
            style.foreground
        } else {
            style.muted
        })
        .when(enabled, |button| button.role(Role::Button))
        .when(!enabled, |button| button.role(Role::Group))
        .aria_label(if loading {
            "Refresh cryptocurrency prices; request in progress"
        } else if !refresh_enabled {
            "Refresh unavailable; no coins are configured"
        } else {
            "Refresh cryptocurrency prices"
        })
        .focusable()
        .tab_stop(enabled)
        .when(enabled, |button| {
            let border = style.border;
            let hover = style.hover;
            button
                .focus_visible(move |s| s.border_1().border_color(border))
                .hover(move |s| s.bg(hover))
                .on_click(move |_, _, _| click_refresh())
                .on_key_down(move |event: &KeyDownEvent, _, cx| {
                    if activate(event) {
                        key_refresh();
                        cx.stop_propagation();
                    }
                })
        })
        .font_family("Material Symbols Rounded")
        .text_size(px(14.))
        .child("refresh")
}

fn render_coin(coin: CryptoCoin, style: &SidebarStyle, history_pending: bool) -> impl IntoElement {
    // The QML `fmtPrice` uses `$---` for zero or missing prices; preserve that
    // unavailable placeholder instead of presenting zero as a live quote.
    let price = coin
        .price_usd
        .filter(|value| value.is_finite() && *value > 0.0);
    let change = price
        .and_then(|_| coin.change_24h)
        .filter(|value| value.is_finite());
    let positive = change.is_some_and(|value| value >= 0.0);
    let change_color = if change.is_some() {
        if positive {
            style.accent
        } else {
            gpui::rgba(0xf85149ff).into()
        }
    } else {
        style.muted
    };
    let symbol = coin.symbol.to_ascii_uppercase();
    let price_text = price.map_or_else(|| "unavailable".to_owned(), format_price);
    let price_display = price.map_or_else(
        || "$---".to_owned(),
        |value| format!("${}", format_price(value)),
    );
    let change_text = change.map_or_else(|| "—".to_owned(), |value| format!("{:.1}%", value.abs()));
    let change_icon = if positive {
        "arrow_drop_up"
    } else {
        "arrow_drop_down"
    };
    let has_sparkline = coin
        .sparkline
        .iter()
        .filter(|value| value.is_finite())
        .count()
        > 1;
    let trend = if has_sparkline && history_pending {
        "historical price trend available; refresh pending"
    } else if has_sparkline {
        "historical price trend available"
    } else if history_pending {
        "historical price trend loading"
    } else {
        "historical price trend unavailable"
    };
    let change_direction = change.map_or_else(
        || "change unavailable".to_owned(),
        |value| {
            if value >= 0.0 {
                format!("increased by {:.1}%", value.abs())
            } else {
                format!("decreased by {:.1}%", value.abs())
            }
        },
    );
    let accessible = format!(
        "{} price {}, 24-hour change {}, {}",
        symbol, price_text, change_direction, trend
    );
    let sparkline = coin.sparkline;
    let line_color = match change {
        Some(value) if value >= 0.0 => style.accent,
        Some(_) => gpui::rgba(0xf85149ff).into(),
        None => style.muted,
    };

    div()
        .id(SharedString::from(format!(
            "ii.crypto-widget.coin.{}",
            coin.id
        )))
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(4.))
        .role(Role::Group)
        .aria_label(SharedString::from(accessible))
        .child(
            div()
                .w(px(36.))
                .flex_shrink_0()
                .text_xs()
                .font_family("monospace")
                .text_color(style.foreground)
                .child(symbol),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(style.foreground)
                .child(price_display),
        )
        .child(render_sparkline(
            &coin.id,
            sparkline,
            line_color,
            history_pending,
        ))
        .child(
            div()
                .w(px(50.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(0.))
                .font_family("Material Symbols Rounded")
                .text_size(px(14.))
                .text_color(change_color)
                .child(if change.is_some() { change_icon } else { "" })
                .child(
                    div()
                        .font_family("sans-serif")
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(change_color)
                        .child(change_text),
                ),
        )
}

fn render_sparkline(
    id: &str,
    values: Vec<f32>,
    color: gpui::Hsla,
    history_pending: bool,
) -> impl IntoElement {
    let values = values
        .into_iter()
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 1.0))
        .collect::<Vec<_>>();
    if values.len() < 2 {
        return div()
            .id(SharedString::from(format!(
                "ii.crypto-widget.coin.{id}.sparkline.unavailable"
            )))
            .w(px(50.))
            .h(px(16.))
            .flex_shrink_0()
            .role(Role::Group)
            .text_xs()
            .text_color(color)
            .aria_label(if history_pending {
                "Historical price trend loading"
            } else {
                "Historical price trend unavailable"
            })
            .child(if history_pending { "…" } else { "" });
    }
    div()
        .id(SharedString::from(format!(
            "ii.crypto-widget.coin.{id}.sparkline"
        )))
        .w(px(50.))
        .h(px(16.))
        .flex_shrink_0()
        .role(Role::Group)
        .aria_label("Historical cryptocurrency prices")
        .child(canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let width = f32::from(bounds.size.width).max(1.0);
                let height = f32::from(bounds.size.height).max(1.0);
                let inset = 1.0_f32;
                let scale_x = (width - 2.0 * inset).max(0.0);
                let scale_y = (height - 2.0 * inset).max(0.0);
                let mut path = PathBuilder::stroke(px(1.5));
                for (index, value) in values.iter().enumerate() {
                    let x = if values.len() == 1 {
                        inset
                    } else {
                        inset + scale_x * index as f32 / (values.len() - 1) as f32
                    };
                    let y = inset + (1.0 - *value) * scale_y;
                    let at = point(bounds.origin.x + px(x), bounds.origin.y + px(y));
                    if index == 0 {
                        path.move_to(at);
                    } else {
                        path.line_to(at);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
            },
        ))
}

fn format_price(price: f64) -> String {
    if price >= 1_000.0 {
        format!("{price:.0}")
    } else if price >= 1.0 {
        format!("{price:.2}")
    } else if price >= 0.01 {
        format!("{price:.4}")
    } else {
        format!("{price:.6}")
    }
}

fn activate(event: &KeyDownEvent) -> bool {
    !event.is_held && matches!(event.keystroke.key.as_str(), "enter" | "space")
}
