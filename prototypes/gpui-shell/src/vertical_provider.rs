//! Async session-bus adapters for ii desktop data.
//! The UI only receives snapshots; every D-Bus request runs from the GPUI task
//! and uses zbus's async-io transport.

use zbus::{Connection, Proxy, zvariant::OwnedValue};

use crate::components::vertical_bar::{
    BatteryChargeState, BatteryState, BatteryStatus, MediaAction, MediaPlayerSnapshot, MediaState,
    MediaStatus, PlayerMediaCommand,
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
    players: Vec<MediaPlayerSnapshot>,
}

impl MprisProvider {
    pub async fn sample(&mut self) -> MediaState {
        let Ok(connection) = self.session_connection().await else {
            clear_mpris_targets(&mut self.active_player, &mut self.players);
            return unavailable_media();
        };
        let (state, player) =
            sample_media(&connection, self.active_player.as_deref(), &self.players).await;
        if state.status == MediaStatus::Unavailable {
            self.session_connection = None;
        }
        self.active_player = player;
        self.players = state.players.clone();
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
        let Some((bus_name, action)) =
            media_action_target(&action, self.active_player.as_deref(), &self.players)
        else {
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
            MediaAction::SeekTo { seconds, track_id } => {
                if !seek_track_matches(&self.players, &bus_name, &track_id) {
                    return;
                }
                if !proxy.get_property::<bool>("CanSeek").await.unwrap_or(false) {
                    return;
                }
                if let Ok(track_id) = zbus::zvariant::OwnedObjectPath::try_from(track_id)
                    && let Ok(position) = i64::try_from(seconds.saturating_mul(1_000_000))
                {
                    let _ = proxy
                        .call_method("SetPosition", &(track_id, position))
                        .await;
                }
            }
            MediaAction::OpenControls
            | MediaAction::CloseControls
            | MediaAction::ForPlayer { .. } => {}
        }
    }
}

fn media_action_target(
    action: &MediaAction,
    active_player: Option<&str>,
    players: &[MediaPlayerSnapshot],
) -> Option<(String, MediaAction)> {
    match action {
        MediaAction::ForPlayer {
            service_bus_name,
            command,
        } => {
            if !players
                .iter()
                .any(|player| player.service_bus_name == *service_bus_name)
            {
                return None;
            }
            let action = match command {
                PlayerMediaCommand::TogglePlayback => MediaAction::TogglePlayback,
                PlayerMediaCommand::Previous => MediaAction::Previous,
                PlayerMediaCommand::Next => MediaAction::Next,
                PlayerMediaCommand::Seek { seconds, track_id } => MediaAction::SeekTo {
                    seconds: *seconds,
                    track_id: track_id.clone(),
                },
            };
            Some((service_bus_name.clone(), action))
        }
        MediaAction::OpenControls | MediaAction::CloseControls => None,
        MediaAction::SeekTo { .. } => None,
        action => active_player
            .filter(|name| !name.is_empty())
            .map(|name| (name.to_owned(), action.clone())),
    }
}

fn seek_track_matches(
    players: &[MediaPlayerSnapshot],
    service_bus_name: &str,
    requested_track_id: &str,
) -> bool {
    players
        .iter()
        .find(|player| player.service_bus_name == service_bus_name)
        .and_then(|player| player.track_id.as_deref())
        == Some(requested_track_id)
}

fn clear_mpris_targets(active_player: &mut Option<String>, players: &mut Vec<MediaPlayerSnapshot>) {
    *active_player = None;
    players.clear();
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
    let time_to_full_seconds = proxy
        .get_property::<i64>("TimeToFull")
        .await
        .ok()
        .and_then(non_negative_seconds);
    let time_to_empty_seconds = proxy
        .get_property::<i64>("TimeToEmpty")
        .await
        .ok()
        .and_then(non_negative_seconds);
    let energy_rate_watts = proxy
        .get_property::<f64>("EnergyRate")
        .await
        .ok()
        .and_then(valid_energy_rate);
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
        time_to_full_seconds,
        time_to_empty_seconds,
        energy_rate_watts,
    }))
}

fn non_negative_seconds(value: i64) -> Option<u64> {
    u64::try_from(value).ok()
}

fn valid_energy_rate(value: f64) -> Option<f64> {
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn unavailable_battery() -> BatteryState {
    BatteryState {
        status: BatteryStatus::Unavailable,
        charge: BatteryChargeState::Unknown,
        percentage: None,
        low_threshold: 20.0,
        time_to_full_seconds: None,
        time_to_empty_seconds: None,
        energy_rate_watts: None,
    }
}

#[cfg(test)]
mod battery_property_tests {
    use super::{non_negative_seconds, valid_energy_rate};

    #[test]
    fn non_negative_seconds_accepts_zero_and_positive_estimates() {
        assert_eq!(non_negative_seconds(0), Some(0));
        assert_eq!(non_negative_seconds(86_400), Some(86_400));
    }

    #[test]
    fn non_negative_seconds_rejects_negative_estimates() {
        assert_eq!(non_negative_seconds(-1), None);
    }

    #[test]
    fn energy_rate_accepts_zero_and_finite_positive_values() {
        assert_eq!(valid_energy_rate(0.0), Some(0.0));
        assert_eq!(valid_energy_rate(12.5), Some(12.5));
    }

    #[test]
    fn energy_rate_rejects_negative_and_non_finite_values() {
        assert_eq!(valid_energy_rate(-0.1), None);
        assert_eq!(valid_energy_rate(f64::NAN), None);
        assert_eq!(valid_energy_rate(f64::INFINITY), None);
    }
}

#[cfg(test)]
mod mpris_snapshot_tests {
    use super::{
        clear_mpris_targets, deduplicate_players, media_action_target, preserve_roster_order,
        seek_track_matches,
    };
    use crate::components::vertical_bar::{
        MediaAction, MediaPlayerSnapshot, MediaStatus, PlayerMediaCommand,
    };

    fn player(name: &str, title: &str) -> MediaPlayerSnapshot {
        MediaPlayerSnapshot {
            service_bus_name: name.to_owned(),
            identity: Some(name.to_owned()),
            title: Some(title.to_owned()),
            artist: None,
            album: None,
            source_url: None,
            art_url: None,
            track_id: Some("/track/1".to_owned()),
            status: MediaStatus::Playing,
            position_seconds: None,
            duration_seconds: None,
            position_microseconds: None,
            duration_microseconds: None,
            can_toggle_playback: true,
            can_previous: true,
            can_next: true,
            can_seek: true,
            can_change_volume: false,
        }
    }

    #[test]
    fn generic_title_dedup_prefers_art_representative_and_preserves_group_order() {
        let first = player("org.mpris.MediaPlayer2.a", "Song");
        let mut duplicate = player("org.mpris.MediaPlayer2.b", "Song (Official Video)");
        duplicate.art_url = Some("file:///cover.jpg".to_owned());
        let other = player("org.mpris.MediaPlayer2.c", "Different");
        let filtered = deduplicate_players(vec![first, duplicate, other]);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].service_bus_name, "org.mpris.MediaPlayer2.b");
        assert_eq!(filtered[1].service_bus_name, "org.mpris.MediaPlayer2.c");
    }

    #[test]
    fn generic_dedup_matches_position_duration_and_exact_nonempty_url() {
        let mut first = player("a", "First");
        let mut by_position = player("b", "Unrelated title");
        by_position.position_seconds = Some(13);
        by_position.duration_seconds = Some(123);
        by_position.position_microseconds = Some(13_000_000);
        by_position.duration_microseconds = Some(123_000_000);
        let mut by_url_left = player("c", "Live stream");
        let mut by_url_right = player("d", "Browser tab");
        by_url_left.source_url = Some("https://example.test/live".to_owned());
        by_url_right.source_url = by_url_left.source_url.clone();
        first.position_seconds = Some(10);
        first.duration_seconds = Some(120);
        first.position_microseconds = Some(10_000_000);
        first.duration_microseconds = Some(120_000_000);
        let filtered = deduplicate_players(vec![first, by_position, by_url_left, by_url_right]);
        assert_eq!(
            filtered
                .iter()
                .map(|player| player.service_bus_name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "c"]
        );
    }

    #[test]
    fn targeted_actions_use_exact_known_bus_and_reject_stale_targets() {
        let players = vec![player("known", "Song")];
        let targeted = MediaAction::ForPlayer {
            service_bus_name: "known".to_owned(),
            command: PlayerMediaCommand::Next,
        };
        assert_eq!(
            media_action_target(&targeted, Some("active"), &players),
            Some(("known".to_owned(), MediaAction::Next))
        );
        let stale = MediaAction::ForPlayer {
            service_bus_name: "gone".to_owned(),
            command: PlayerMediaCommand::Previous,
        };
        assert_eq!(media_action_target(&stale, Some("active"), &players), None);
    }

    #[test]
    fn legacy_actions_continue_to_use_active_player() {
        let players = vec![player("one", "Song"), player("two", "Other")];
        assert_eq!(
            media_action_target(&MediaAction::TogglePlayback, Some("one"), &players),
            Some(("one".to_owned(), MediaAction::TogglePlayback))
        );
        assert_eq!(
            media_action_target(&MediaAction::OpenControls, Some("one"), &players),
            None
        );
    }

    #[test]
    fn roster_reuses_previous_order_then_appends_new_bus_entries_in_source_order() {
        let previous = vec![player("first", "A"), player("second", "B")];
        let incoming = vec![
            player("new-z", "Z"),
            player("second", "B"),
            player("first", "A"),
            player("new-a", "A"),
        ];
        let ordered = preserve_roster_order(incoming, &previous);
        assert_eq!(
            ordered
                .iter()
                .map(|player| player.service_bus_name.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second", "new-z", "new-a"]
        );
    }

    #[test]
    fn targeted_seek_resolves_only_against_current_player_roster() {
        let players = vec![player("known", "Song")];
        let seek = MediaAction::ForPlayer {
            service_bus_name: "known".to_owned(),
            command: PlayerMediaCommand::Seek {
                seconds: 42,
                track_id: "/track/1".to_owned(),
            },
        };
        assert_eq!(
            media_action_target(&seek, Some("active"), &players),
            Some((
                "known".to_owned(),
                MediaAction::SeekTo {
                    seconds: 42,
                    track_id: "/track/1".to_owned(),
                }
            ))
        );
        let stale = MediaAction::ForPlayer {
            service_bus_name: "gone".to_owned(),
            command: PlayerMediaCommand::Seek {
                seconds: 42,
                track_id: "/track/1".to_owned(),
            },
        };
        assert_eq!(media_action_target(&stale, Some("active"), &players), None);
    }

    #[test]
    fn targeted_seek_rejects_same_bus_when_current_track_changed() {
        let requested_track_id = "/track/1";
        let mut current = player("known", "New song");
        current.track_id = Some("/track/2".to_owned());

        assert!(!seek_track_matches(&[current], "known", requested_track_id));
    }

    #[test]
    fn connection_failure_clears_active_and_player_targets() {
        let mut active_player = Some("known".to_owned());
        let mut players = vec![player("known", "Old song")];
        clear_mpris_targets(&mut active_player, &mut players);

        let stale_action = MediaAction::ForPlayer {
            service_bus_name: "known".to_owned(),
            command: PlayerMediaCommand::Seek {
                seconds: 42,
                track_id: "/track/1".to_owned(),
            },
        };
        assert_eq!(active_player, None);
        assert!(players.is_empty());
        assert_eq!(
            media_action_target(&stale_action, active_player.as_deref(), &players),
            None
        );
        assert!(!seek_track_matches(&players, "known", "/track/1"));
    }
}

async fn sample_media(
    connection: &Connection,
    previously_active: Option<&str>,
    previous_roster: &[MediaPlayerSnapshot],
) -> (MediaState, Option<String>) {
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
    let candidates: Vec<_> = names
        .into_iter()
        .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
        .collect();
    if candidates.is_empty() {
        return (empty_media(), None);
    }

    let mut snapshots = Vec::new();
    for bus_name in candidates {
        let Ok(proxy) = player_proxy(connection, &bus_name).await else {
            continue;
        };
        if let Some(snapshot) = read_player(connection, &bus_name, &proxy).await {
            snapshots.push(snapshot);
        }
    }
    if snapshots.is_empty() {
        return (unavailable_media(), None);
    }

    let snapshots = preserve_roster_order(snapshots, previous_roster);
    let snapshots = deduplicate_players(snapshots);
    let active = snapshots
        .iter()
        .find(|player| player.status == MediaStatus::Playing)
        .or_else(|| {
            previously_active.and_then(|name| {
                snapshots
                    .iter()
                    .find(|player| player.service_bus_name == name)
            })
        })
        .or_else(|| snapshots.first());
    let Some(active) = active else {
        return (empty_media(), None);
    };
    let active = active.clone();
    let active_name = active.service_bus_name.clone();
    (
        media_state_from_player(&active, snapshots),
        Some(active_name),
    )
}

/// Keep the live roster stable when ListNames returns an existing service in
/// a different array position, while appending newly observed services in
/// the order returned by the bus.
fn preserve_roster_order(
    mut incoming: Vec<MediaPlayerSnapshot>,
    previous: &[MediaPlayerSnapshot],
) -> Vec<MediaPlayerSnapshot> {
    let mut ordered = Vec::with_capacity(incoming.len());
    for existing in previous {
        if let Some(index) = incoming
            .iter()
            .position(|player| player.service_bus_name == existing.service_bus_name)
        {
            ordered.push(incoming.remove(index));
        }
    }
    ordered.extend(incoming);
    ordered
}

async fn player_proxy<'a>(
    connection: &'a Connection,
    bus_name: &'a str,
) -> zbus::Result<Proxy<'a>> {
    Proxy::new(connection, bus_name, MPRIS_ROOT, MPRIS_PLAYER).await
}

async fn read_player(
    connection: &Connection,
    bus_name: &str,
    proxy: &Proxy<'_>,
) -> Option<MediaPlayerSnapshot> {
    let playback = proxy.get_property::<String>("PlaybackStatus").await.ok()?;
    let root = Proxy::new(connection, bus_name, MPRIS_ROOT, "org.mpris.MediaPlayer2")
        .await
        .ok()?;
    let identity = root
        .get_property::<String>("Identity")
        .await
        .ok()
        .filter(|value| !value.trim().is_empty());
    let metadata = proxy
        .get_property::<std::collections::HashMap<String, OwnedValue>>("Metadata")
        .await
        .unwrap_or_default();
    let title = metadata
        .get("xesam:title")
        .and_then(|value| <String>::try_from(value.try_clone().ok()?).ok())
        .filter(|value| !value.trim().is_empty());
    let album = metadata
        .get("xesam:album")
        .and_then(|value| <String>::try_from(value.try_clone().ok()?).ok())
        .filter(|value| !value.trim().is_empty());
    let source_url = metadata
        .get("xesam:url")
        .and_then(|value| <String>::try_from(value.try_clone().ok()?).ok())
        .filter(|value| !value.trim().is_empty());
    let art_url = metadata
        .get("mpris:artUrl")
        .and_then(|value| <String>::try_from(value.try_clone().ok()?).ok())
        .filter(|value| !value.trim().is_empty());
    let track_id = metadata
        .get("mpris:trackid")
        .and_then(|value| <zbus::zvariant::OwnedObjectPath>::try_from(value.try_clone().ok()?).ok())
        .map(|value| value.to_string());
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
    let can_seek = can_control
        && track_id.is_some()
        && proxy.get_property::<bool>("CanSeek").await.unwrap_or(false);
    Some(MediaPlayerSnapshot {
        service_bus_name: bus_name.to_owned(),
        identity,
        title,
        artist,
        album,
        source_url,
        art_url,
        track_id,
        status: match playback.as_str() {
            "Playing" => MediaStatus::Playing,
            "Paused" => MediaStatus::Paused,
            _ => MediaStatus::Stopped,
        },
        position_seconds: position.and_then(|value| non_negative_seconds(value / 1_000_000)),
        duration_seconds: length.and_then(|value| non_negative_seconds(value / 1_000_000)),
        position_microseconds: position.and_then(non_negative_seconds),
        duration_microseconds: length.and_then(non_negative_seconds),
        can_toggle_playback,
        can_previous,
        can_next,
        can_seek,
        can_change_volume,
    })
}

fn media_state_from_player(
    player: &MediaPlayerSnapshot,
    players: Vec<MediaPlayerSnapshot>,
) -> MediaState {
    let mut state = player.as_media_state();
    state.players = players;
    state
}

/// Implements the generic QML duplicate grouping. QML-only player eligibility
/// and YtMusic ordering heuristics remain in the QML controller and are not
/// inferred here. Generic duplicate filtering intentionally remains enabled
/// when `filterDuplicatePlayers` is false, matching QML's separate controls.
pub fn deduplicate_players(players: Vec<MediaPlayerSnapshot>) -> Vec<MediaPlayerSnapshot> {
    let mut output = Vec::new();
    let mut used = vec![false; players.len()];
    for index in 0..players.len() {
        if used[index] {
            continue;
        }
        let first = &players[index];
        let mut group = vec![index];
        for candidate_index in (index + 1)..players.len() {
            if used[candidate_index] {
                continue;
            }
            if players_are_duplicates(first, &players[candidate_index]) {
                group.push(candidate_index);
            }
        }
        let chosen = group
            .iter()
            .copied()
            .find(|candidate| {
                players[*candidate]
                    .art_url
                    .as_ref()
                    .is_some_and(|art| !art.is_empty())
            })
            .unwrap_or(index);
        for candidate in group {
            used[candidate] = true;
        }
        output.push(players[chosen].clone());
    }
    output
}

fn players_are_duplicates(first: &MediaPlayerSnapshot, second: &MediaPlayerSnapshot) -> bool {
    let title_match = match (first.title.as_deref(), second.title.as_deref()) {
        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => a.contains(b) || b.contains(a),
        _ => false,
    };
    let position_match = first
        .position_microseconds
        .zip(second.position_microseconds)
        .zip(
            first
                .duration_microseconds
                .zip(second.duration_microseconds),
        )
        .is_some_and(|((a_pos, b_pos), (a_len, b_len))| {
            a_len > 0
                && b_len > 0
                && a_pos.abs_diff(b_pos) <= 3_000_000
                && a_len.abs_diff(b_len) <= 3_000_000
        });
    let url_match = first
        .source_url
        .as_ref()
        .is_some_and(|url| !url.is_empty() && second.source_url.as_ref() == Some(url));
    title_match || position_match || url_match
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
        players: Vec::new(),
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
        players: Vec::new(),
    }
}
