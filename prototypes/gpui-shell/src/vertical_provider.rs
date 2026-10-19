//! Async session-bus adapters for ii desktop data.
//! The UI only receives snapshots; every D-Bus request runs from the GPUI task
//! and uses zbus's async-io transport.

use zbus::{Connection, Proxy, zvariant::OwnedValue};

use crate::components::vertical_bar::{
    BatteryChargeState, BatteryState, BatteryStatus, MediaAction, MediaState, MediaStatus,
};

const UPOWER_SERVICE: &str = "org.freedesktop.UPower";
const UPOWER_DEVICE: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const MPRIS_ROOT: &str = "/org/mpris/MediaPlayer2";
const MPRIS_PLAYER: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Default)]
pub struct VerticalProvider {
    system_connection: Option<Connection>,
    battery_low: f64,
}

impl VerticalProvider {
    pub fn new(battery_low: f64) -> Self {
        Self {
            battery_low: battery_low.clamp(0.0, 100.0),
            system_connection: None,
        }
    }

    pub async fn sample(&mut self) -> Option<BatteryState> {
        match self.system_connection().await {
            Ok(connection) => match sample_battery(&connection, self.battery_low).await {
                Ok(state) => state,
                Err(()) => Some(unavailable_battery()),
            },
            Err(_) => Some(unavailable_battery()),
        }
    }
}

#[derive(Default)]
pub struct MprisProvider {
    session_connection: Option<Connection>,
    active_player: Option<String>,
}

impl MprisProvider {
    pub async fn sample(&mut self) -> MediaState {
        let Ok(connection) = self.session_connection().await else {
            self.active_player = None;
            return unavailable_media();
        };
        let (state, player) = sample_media(&connection).await;
        if state.status == MediaStatus::Unavailable {
            self.session_connection = None;
        }
        self.active_player = player;
        state
    }

    async fn session_connection(&mut self) -> zbus::Result<Connection> {
        if let Some(connection) = &self.session_connection {
            return Ok(connection.clone());
        }
        let connection = Connection::session().await?;
        self.session_connection = Some(connection.clone());
        Ok(connection)
    }

    pub async fn perform_media_action(&mut self, action: MediaAction) {
        let Some(bus_name) = self.active_player.clone() else {
            return;
        };
        let Ok(connection) = self.session_connection().await else {
            self.session_connection = None;
            return;
        };
        let Ok(proxy) = player_proxy(&connection, &bus_name).await else {
            return;
        };
        let can_control = proxy
            .get_property::<bool>("CanControl")
            .await
            .unwrap_or(false);
        if !can_control {
            return;
        }
        match action {
            MediaAction::TogglePlayback => {
                let status = proxy.get_property::<String>("PlaybackStatus").await;
                match status.as_deref() {
                    Ok("Playing")
                        if proxy
                            .get_property::<bool>("CanPause")
                            .await
                            .unwrap_or(false) =>
                    {
                        let _ = proxy.call_method("Pause", &()).await;
                    }
                    Ok("Paused" | "Stopped")
                        if proxy.get_property::<bool>("CanPlay").await.unwrap_or(false) =>
                    {
                        let _ = proxy.call_method("Play", &()).await;
                    }
                    _ => {}
                }
            }
            MediaAction::Previous => {
                if proxy
                    .get_property::<bool>("CanGoPrevious")
                    .await
                    .unwrap_or(false)
                {
                    let _ = proxy.call_method("Previous", &()).await;
                }
            }
            MediaAction::Next => {
                if proxy
                    .get_property::<bool>("CanGoNext")
                    .await
                    .unwrap_or(false)
                {
                    let _ = proxy.call_method("Next", &()).await;
                }
            }
            MediaAction::VolumeDelta(delta) => {
                if !proxy
                    .get_property::<bool>("CanControl")
                    .await
                    .unwrap_or(false)
                {
                    return;
                }
                if let Ok(volume) = proxy.get_property::<f64>("Volume").await {
                    let _ = proxy
                        .set_property("Volume", (volume + f64::from(delta)).clamp(0.0, 1.0))
                        .await;
                }
            }
            MediaAction::OpenControls => {}
        }
    }
}

impl VerticalProvider {
    async fn system_connection(&mut self) -> zbus::Result<Connection> {
        if let Some(connection) = &self.system_connection {
            return Ok(connection.clone());
        }
        let connection = Connection::system().await?;
        self.system_connection = Some(connection.clone());
        Ok(connection)
    }
}

async fn sample_battery(
    connection: &Connection,
    low_threshold: f64,
) -> Result<Option<BatteryState>, ()> {
    let proxy = Proxy::new(
        connection,
        UPOWER_SERVICE,
        UPOWER_DEVICE,
        "org.freedesktop.UPower.Device",
    )
    .await
    .map_err(|_| ())?;
    if !proxy
        .get_property::<bool>("IsPresent")
        .await
        .map_err(|_| ())?
    {
        return Ok(None);
    }
    let raw_state = proxy.get_property::<u32>("State").await.map_err(|_| ())?;
    let percentage = proxy
        .get_property::<f64>("Percentage")
        .await
        .ok()
        .filter(|value| value.is_finite())
        .map(|value| value.round().clamp(0.0, 100.0) as u8);
    let state = match raw_state {
        1 => BatteryChargeState::Charging,
        5 => BatteryChargeState::Unknown,
        2 | 6 => {
            if percentage.is_some_and(|value| value <= 10) {
                BatteryChargeState::Critical
            } else {
                BatteryChargeState::Discharging
            }
        }
        3 => BatteryChargeState::Critical,
        4 => BatteryChargeState::Full,
        _ => BatteryChargeState::Unknown,
    };
    Ok(Some(BatteryState {
        status: BatteryStatus::Available,
        charge: state,
        percentage,
        low_threshold,
    }))
}

fn unavailable_battery() -> BatteryState {
    BatteryState {
        status: BatteryStatus::Unavailable,
        charge: BatteryChargeState::Unknown,
        percentage: None,
        low_threshold: 20.0,
    }
}

async fn sample_media(connection: &Connection) -> (MediaState, Option<String>) {
    let Ok(dbus) = Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await
    else {
        return (unavailable_media(), None);
    };
    let Ok(names) = dbus.call_method("ListNames", &()).await else {
        return (unavailable_media(), None);
    };
    let Ok(names) = names.body().deserialize::<Vec<String>>() else {
        return (unavailable_media(), None);
    };
    let mut candidates: Vec<_> = names
        .into_iter()
        .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
        .collect();
    candidates.sort();
    if candidates.is_empty() {
        return (empty_media(), None);
    }

    let mut first_candidate = None;
    for bus_name in candidates {
        let Ok(proxy) = player_proxy(connection, &bus_name).await else {
            continue;
        };
        first_candidate.get_or_insert_with(|| bus_name.clone());
        if let Some(state) = read_player(&proxy).await {
            if state.status == MediaStatus::Playing {
                return (state, Some(bus_name));
            }
        }
    }
    if let Some(bus_name) = first_candidate {
        if let Ok(proxy) = player_proxy(connection, &bus_name).await {
            if let Some(state) = read_player(&proxy).await {
                return (state, Some(bus_name));
            }
        }
        return (unavailable_media(), None);
    }
    (unavailable_media(), None)
}

async fn player_proxy<'a>(
    connection: &'a Connection,
    bus_name: &'a str,
) -> zbus::Result<Proxy<'a>> {
    Proxy::new(connection, bus_name, MPRIS_ROOT, MPRIS_PLAYER).await
}

async fn read_player(proxy: &Proxy<'_>) -> Option<MediaState> {
    let playback = proxy.get_property::<String>("PlaybackStatus").await.ok()?;
    let metadata = proxy
        .get_property::<std::collections::HashMap<String, OwnedValue>>("Metadata")
        .await
        .unwrap_or_default();
    let title = metadata
        .get("xesam:title")
        .and_then(|value| <String>::try_from(value.try_clone().ok()?).ok())
        .filter(|value| !value.trim().is_empty());
    let artist = metadata
        .get("xesam:artist")
        .and_then(|value| <Vec<String>>::try_from(value.try_clone().ok()?).ok())
        .and_then(|values| values.into_iter().find(|value| !value.trim().is_empty()));
    let length = metadata
        .get("mpris:length")
        .and_then(|value| <i64>::try_from(value).ok())
        .filter(|length| *length > 0);
    let position = proxy.get_property::<i64>("Position").await.ok();
    let can_control = proxy
        .get_property::<bool>("CanControl")
        .await
        .unwrap_or(false);
    let can_play = proxy.get_property::<bool>("CanPlay").await.unwrap_or(false);
    let can_pause = proxy
        .get_property::<bool>("CanPause")
        .await
        .unwrap_or(false);
    let can_toggle_playback = can_control
        && match playback.as_str() {
            "Playing" => can_pause,
            "Paused" | "Stopped" => can_play,
            _ => false,
        };
    let can_previous = can_control
        && proxy
            .get_property::<bool>("CanGoPrevious")
            .await
            .unwrap_or(false);
    let can_next = can_control
        && proxy
            .get_property::<bool>("CanGoNext")
            .await
            .unwrap_or(false);
    let can_change_volume = can_control && proxy.get_property::<f64>("Volume").await.is_ok();
    let position_percent = length.zip(position).map(|(length, position)| {
        (position.max(0) as f64 / length as f64 * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8
    });
    Some(MediaState {
        status: match playback.as_str() {
            "Playing" => MediaStatus::Playing,
            "Paused" => MediaStatus::Paused,
            _ => MediaStatus::Stopped,
        },
        title,
        artist,
        position_percent,
        can_toggle_playback,
        can_previous,
        can_next,
        can_change_volume,
    })
}

pub fn unavailable_media() -> MediaState {
    MediaState {
        status: MediaStatus::Unavailable,
        title: None,
        artist: None,
        position_percent: None,
        can_toggle_playback: false,
        can_previous: false,
        can_next: false,
        can_change_volume: false,
    }
}

fn empty_media() -> MediaState {
    MediaState {
        status: MediaStatus::NoPlayer,
        title: None,
        artist: None,
        position_percent: None,
        can_toggle_playback: false,
        can_previous: false,
        can_next: false,
        can_change_volume: false,
    }
}
