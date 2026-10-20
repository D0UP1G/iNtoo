//! Opt-in owner for the freedesktop notification D-Bus interface.
//!
//! This is deliberately only the live protocol boundary. It does not claim
//! history is loaded or that iNtoo notification policy has been applied.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use async_channel::{Receiver, Sender};
use futures_lite::future::{block_on, race};
use image::{DynamicImage, ImageBuffer, ImageFormat as EncoderImageFormat};
use zbus::message::Header;
use zbus::{
    connection,
    fdo::{RequestNameFlags, RequestNameReply},
    interface,
    object_server::SignalEmitter,
    zvariant::{OwnedValue, Structure},
};

const SERVICE_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
const MAX_NOTIFICATION_IMAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_NOTIFICATION_IMAGE_DIMENSION: u32 = 4096;
const MAX_NOTIFICATION_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_NOTIFICATION_IMAGE_PNG_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationImage {
    /// Local resource supplied by the sender. Network URLs are rejected because
    /// GPUI's URI image loader performs outbound HTTP requests.
    Resource(String),
    /// Standard image-data payload normalized to PNG after validating bounds.
    Png(Arc<[u8]>),
}

fn parse_notification_image(hints: &HashMap<String, OwnedValue>) -> Option<NotificationImage> {
    for key in ["image-data", "image_data"] {
        if let Some(value) = hints.get(key)
            && let Some(png) = parse_image_data(value)
        {
            return Some(NotificationImage::Png(png.into()));
        }
    }
    for key in ["image-path", "image_path", "image-url", "image_url"] {
        if let Some(value) = hints.get(key)
            && let Ok(resource) = String::try_from(value.try_clone().ok()?)
            && let Some(resource) = validate_image_resource(resource)
        {
            return Some(NotificationImage::Resource(resource));
        }
    }
    None
}

fn validate_image_resource(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        return None;
    }
    if value.starts_with("file://") || value.starts_with('/') {
        return Some(value.to_owned());
    }
    None
}

fn parse_image_data(value: &OwnedValue) -> Option<Vec<u8>> {
    let structure = Structure::try_from(value.try_clone().ok()?).ok()?;
    let (width, height, rowstride, has_alpha, bits_per_sample, channels, data) =
        <(i32, i32, i32, bool, i32, i32, Vec<u8>)>::try_from(structure).ok()?;
    encode_notification_image_data(
        width,
        height,
        rowstride,
        has_alpha,
        bits_per_sample,
        channels,
        &data,
    )
}

fn encode_notification_image_data(
    width: i32,
    height: i32,
    rowstride: i32,
    has_alpha: bool,
    bits_per_sample: i32,
    channels: i32,
    data: &[u8],
) -> Option<Vec<u8>> {
    let (width, height, rowstride) = (
        u32::try_from(width).ok()?,
        u32::try_from(height).ok()?,
        usize::try_from(rowstride).ok()?,
    );
    let channel_count = usize::try_from(channels).ok()?;
    if width == 0
        || height == 0
        || width > MAX_NOTIFICATION_IMAGE_DIMENSION
        || height > MAX_NOTIFICATION_IMAGE_DIMENSION
        || u64::from(width).checked_mul(u64::from(height))? > MAX_NOTIFICATION_IMAGE_PIXELS
        || bits_per_sample != 8
        || channel_count != if has_alpha { 4 } else { 3 }
        || data.len() > MAX_NOTIFICATION_IMAGE_BYTES
    {
        return None;
    }
    let minimum_row = usize::try_from(width).ok()?.checked_mul(channel_count)?;
    let expected_data_len = rowstride.checked_mul(usize::try_from(height).ok()?)?;
    if rowstride < minimum_row
        || rowstride > MAX_NOTIFICATION_IMAGE_BYTES
        || expected_data_len > MAX_NOTIFICATION_IMAGE_BYTES
        || data.len() != expected_data_len
    {
        return None;
    }

    let pixels = width.checked_mul(height)?;
    let rgba_len = usize::try_from(pixels).ok()?.checked_mul(4)?;
    let mut rgba = Vec::with_capacity(rgba_len);
    for y in 0..usize::try_from(height).ok()? {
        let row = &data[y.checked_mul(rowstride)?..][..minimum_row];
        for sample in row.chunks_exact(channel_count) {
            rgba.extend_from_slice(&[
                sample[0],
                sample[1],
                sample[2],
                if has_alpha { sample[3] } else { 255 },
            ]);
        }
    }
    let image = if has_alpha {
        DynamicImage::ImageRgba8(ImageBuffer::from_raw(width, height, rgba)?)
    } else {
        let mut rgb = Vec::with_capacity(usize::try_from(pixels).ok()?.checked_mul(3)?);
        for pixel in rgba.chunks_exact(4) {
            rgb.extend_from_slice(&pixel[..3]);
        }
        DynamicImage::ImageRgb8(ImageBuffer::from_raw(width, height, rgb)?)
    };
    let mut encoded = std::io::Cursor::new(Vec::new());
    image.write_to(&mut encoded, EncoderImageFormat::Png).ok()?;
    let png = encoded.into_inner();
    (png.len() <= MAX_NOTIFICATION_IMAGE_PNG_BYTES).then_some(png)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationServiceStatus {
    Disabled,
    Starting,
    OwnerReady,
    Stopped,
    Unavailable(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationSnapshotState {
    Loading,
    Unavailable,
    /// The D-Bus name is owned and the in-memory session list is authoritative.
    /// This does not include persisted history or unread/read policy.
    ReadySessionOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveNotification {
    pub id: u32,
    /// Unique bus name of the sending application, used to validate replacement IDs.
    pub sender: String,
    pub app_name: String,
    pub app_icon: String,
    pub image: Option<NotificationImage>,
    pub summary: String,
    pub body: String,
    /// Pairs are (action identifier, user-facing label), as defined by the protocol.
    pub actions: Vec<(String, String)>,
    pub urgency: NotificationUrgency,
    /// Sender requested timeout in milliseconds; negative means server default.
    pub expire_timeout_ms: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotificationUrgency {
    Low,
    #[default]
    Normal,
    Critical,
}

#[derive(Clone, Debug)]
pub struct NotificationSnapshot {
    pub service: NotificationServiceStatus,
    /// Session-only readiness is distinct from restored history readiness.
    pub state: NotificationSnapshotState,
    /// Protocol-live items only; not a replacement for persisted history.
    pub live: Vec<LiveNotification>,
}

#[derive(Clone, Debug)]
pub enum NotificationEvent {
    ServiceStatus(NotificationServiceStatus),
    Added(LiveNotification),
    Replaced {
        old_id: u32,
        notification: LiveNotification,
    },
    Closed {
        id: u32,
        reason: u32,
    },
    ActionInvoked {
        id: u32,
        action_id: String,
    },
}

#[derive(Clone)]
pub struct NotificationServiceHandle {
    state: Arc<Mutex<ServiceState>>,
    commands: Sender<ServiceCommand>,
    shutdown: Sender<()>,
    events: Sender<NotificationEvent>,
}

enum ServiceCommand {
    Notify {
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<(String, String)>,
        urgency: NotificationUrgency,
        expire_timeout_ms: i32,
        image: Option<NotificationImage>,
        sender: String,
        reply: Sender<Result<u32, String>>,
    },
    Dismiss {
        id: u32,
        reason: u32,
    },
    CancelTimeout {
        id: u32,
    },
    InvokeAction {
        id: u32,
        action_id: String,
    },
    CloseFromSender {
        id: u32,
        reply: Sender<CloseResult>,
    },
    Expire {
        id: u32,
        generation: u64,
    },
}

enum ExpiryCommand {
    Schedule {
        id: u32,
        generation: u64,
        deadline: Instant,
    },
    Cancel {
        id: u32,
    },
    Shutdown,
}

enum CloseResult {
    Closed,
    UnknownId,
    Failed(String),
}

enum WorkerInput {
    Command(ServiceCommand),
    Shutdown,
}

#[derive(Default)]
struct ServiceState {
    service: Option<NotificationServiceStatus>,
    next_id: u32,
    live: HashMap<u32, LiveNotification>,
    expirations: HashMap<u32, (Instant, u64)>,
    next_expiration_generation: u64,
}

impl NotificationServiceHandle {
    /// Starts only when explicitly opted in. The GPUI binary never takes over
    /// the desktop notification name by default.
    pub fn start_if_opted_in() -> (Self, Receiver<NotificationEvent>) {
        let (events, receiver) = async_channel::unbounded();
        let (commands, command_receiver) = async_channel::unbounded();
        let (shutdown, shutdown_receiver) = async_channel::bounded(1);
        let state = Arc::new(Mutex::new(ServiceState {
            service: Some(NotificationServiceStatus::Disabled),
            next_id: 1,
            live: HashMap::new(),
            expirations: HashMap::new(),
            next_expiration_generation: 1,
        }));
        let handle = Self {
            state: state.clone(),
            commands: commands.clone(),
            shutdown: shutdown.clone(),
            events: events.clone(),
        };

        if std::env::var_os("INTOO_GPUI_OWN_NOTIFICATIONS").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return (handle, receiver);
        }

        set_status(&state, &events, NotificationServiceStatus::Starting);
        let spawn = thread::Builder::new()
            .name("intoo-gpui-notifications".into())
            .spawn(move || {
                if let Err(error) = block_on(run_service(
                    state.clone(),
                    events.clone(),
                    commands.clone(),
                    command_receiver,
                    shutdown_receiver,
                )) {
                    set_status(
                        &state,
                        &events,
                        NotificationServiceStatus::Unavailable(error.to_string()),
                    );
                }
            });
        if let Err(error) = spawn {
            set_status(
                &handle.state,
                &handle.events,
                NotificationServiceStatus::Unavailable(error.to_string()),
            );
        }

        (handle, receiver)
    }

    pub fn snapshot(&self) -> NotificationSnapshot {
        let state = self.state.lock().expect("notification state poisoned");
        let service = state
            .service
            .clone()
            .unwrap_or(NotificationServiceStatus::Starting);
        let snapshot_state = match service {
            NotificationServiceStatus::Disabled
            | NotificationServiceStatus::Stopped
            | NotificationServiceStatus::Unavailable(_) => NotificationSnapshotState::Unavailable,
            NotificationServiceStatus::Starting => NotificationSnapshotState::Loading,
            NotificationServiceStatus::OwnerReady => NotificationSnapshotState::ReadySessionOnly,
        };
        let mut live = state.live.values().cloned().collect::<Vec<_>>();
        live.sort_by_key(|item| item.id);
        NotificationSnapshot {
            service,
            state: snapshot_state,
            live,
        }
    }

    /// Queues one live-item dismissal to the serialized service worker.
    pub fn dismiss(&self, id: u32, reason: u32) {
        let _ = self
            .commands
            .send_blocking(ServiceCommand::Dismiss { id, reason });
    }

    /// Cancels the current automatic expiry for a live notification.
    ///
    /// Cancellation is idempotent. The expiration is invalidated under the
    /// state lock before the scheduler is notified, so an already queued stale
    /// expiration command cannot close the notification afterward.
    pub fn cancel_timeout(&self, id: u32) {
        {
            let mut state = self.state.lock().expect("notification state poisoned");
            cancel_notification_timeout(&mut state, id);
        }
        let _ = self
            .commands
            .send_blocking(ServiceCommand::CancelTimeout { id });
    }

    /// Invokes only an action retained from the live sender session.
    pub fn invoke_action(&self, id: u32, action_id: &str) -> bool {
        let found = self
            .state
            .lock()
            .expect("notification state poisoned")
            .live
            .get(&id)
            .is_some_and(|item| item.actions.iter().any(|(key, _)| key == action_id));
        if !found {
            return false;
        }
        self.commands
            .send_blocking(ServiceCommand::InvokeAction {
                id,
                action_id: action_id.to_owned(),
            })
            .is_ok()
    }

    /// Releases the freedesktop name and stops the worker. Safe to call more than once.
    pub fn shutdown(&self) {
        let _ = self.shutdown.try_send(());
    }
}

fn set_status(
    state: &Arc<Mutex<ServiceState>>,
    events: &Sender<NotificationEvent>,
    status: NotificationServiceStatus,
) {
    state.lock().expect("notification state poisoned").service = Some(status.clone());
    let _ = events.send_blocking(NotificationEvent::ServiceStatus(status));
}

async fn run_service(
    state: Arc<Mutex<ServiceState>>,
    events: Sender<NotificationEvent>,
    commands_sender: Sender<ServiceCommand>,
    commands: Receiver<ServiceCommand>,
    shutdown: Receiver<()>,
) -> zbus::Result<()> {
    let iface = NotificationInterface {
        state: state.clone(),
        events: events.clone(),
        commands: commands_sender.clone(),
    };
    // Register the interface before requesting ownership, then explicitly request
    // DO_NOT_QUEUE without either replacement flag. Only a confirmed primary
    // owner may publish OwnerReady.
    let connection = connection::Builder::session()?
        .serve_at(OBJECT_PATH, iface)?
        .build()
        .await?;
    let ownership = connection
        .request_name_with_flags(SERVICE_NAME, RequestNameFlags::DoNotQueue.into())
        .await?;
    if !matches!(ownership, RequestNameReply::PrimaryOwner) {
        return Err(zbus::Error::NameTaken);
    }
    let (expiry_commands, expiry_receiver) = mpsc::channel();
    let expiry_worker = thread::Builder::new()
        .name("intoo-gpui-notification-expiry".into())
        .spawn(move || run_expiry_scheduler(expiry_receiver, commands_sender.clone()))
        .map_err(zbus::Error::from)?;
    set_status(&state, &events, NotificationServiceStatus::OwnerReady);
    loop {
        let input = race(
            async {
                commands
                    .recv()
                    .await
                    .map(WorkerInput::Command)
                    .unwrap_or(WorkerInput::Shutdown)
            },
            async {
                let _ = shutdown.recv().await;
                WorkerInput::Shutdown
            },
        )
        .await;
        match input {
            WorkerInput::Shutdown => {
                connection.release_name(SERVICE_NAME).await?;
                let _ = expiry_commands.send(ExpiryCommand::Shutdown);
                let _ = expiry_worker.join();
                set_status(&state, &events, NotificationServiceStatus::Stopped);
                break;
            }
            WorkerInput::Command(ServiceCommand::Notify {
                app_name,
                replaces_id,
                app_icon,
                summary,
                body,
                actions,
                urgency,
                expire_timeout_ms,
                image,
                sender,
                reply,
            }) => {
                let (result, expiry_update) = {
                    let mut state = state.lock().expect("notification state poisoned");
                    let replacing = replaces_id != 0
                        && state
                            .live
                            .get(&replaces_id)
                            .is_some_and(|existing| existing.sender == sender);
                    let id = match if replacing {
                        Some(replaces_id)
                    } else {
                        allocate_notification_id(&mut state)
                    } {
                        Some(id) => id,
                        None => {
                            drop(state);
                            let _ = reply
                                .send(Err("No notification IDs available".into()))
                                .await;
                            continue;
                        }
                    };
                    let notification = LiveNotification {
                        id,
                        sender,
                        app_name,
                        app_icon,
                        image,
                        summary,
                        body,
                        actions,
                        urgency,
                        expire_timeout_ms,
                    };
                    let expiry_timeout_ms =
                        effective_expiration_timeout_ms(expire_timeout_ms, urgency);
                    let expiry = if let Some(timeout_ms) = expiry_timeout_ms {
                        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
                        let generation = state.next_expiration_generation.max(1);
                        state.next_expiration_generation = generation.wrapping_add(1).max(1);
                        state.expirations.insert(id, (deadline, generation));
                        Some((id, deadline, generation))
                    } else {
                        state.expirations.remove(&id);
                        None
                    };
                    state.live.insert(id, notification.clone());
                    let event = if replacing {
                        NotificationEvent::Replaced {
                            old_id: id,
                            notification,
                        }
                    } else {
                        NotificationEvent::Added(notification)
                    };
                    drop(state);
                    let _ = events.send(event).await;
                    (Ok(id), expiry)
                };
                if let Some((id, deadline, generation)) = expiry_update {
                    let _ = expiry_commands.send(ExpiryCommand::Schedule {
                        id,
                        deadline,
                        generation,
                    });
                } else {
                    // Also removes a prior deadline when a replacement requests zero
                    // or delegates expiry to the server-default policy (-1).
                    // For a brand-new item this is a harmless no-op.
                    if let Ok(id) = result.as_ref() {
                        let _ = expiry_commands.send(ExpiryCommand::Cancel { id: *id });
                    }
                }
                let _ = reply.send(result).await;
            }
            WorkerInput::Command(ServiceCommand::Dismiss { id, reason }) => {
                if !state
                    .lock()
                    .expect("notification state poisoned")
                    .live
                    .contains_key(&id)
                {
                    continue;
                }
                let emitter = SignalEmitter::new(&connection, OBJECT_PATH)?;
                NotificationInterface::notification_closed(&emitter, id, reason).await?;
                {
                    let mut state = state.lock().expect("notification state poisoned");
                    state.live.remove(&id);
                    state.expirations.remove(&id);
                }
                let _ = expiry_commands.send(ExpiryCommand::Cancel { id });
                let _ = events.send(NotificationEvent::Closed { id, reason }).await;
            }
            WorkerInput::Command(ServiceCommand::CancelTimeout { id }) => {
                let _ = expiry_commands.send(ExpiryCommand::Cancel { id });
            }
            WorkerInput::Command(ServiceCommand::InvokeAction { id, action_id }) => {
                let exists = state
                    .lock()
                    .expect("notification state poisoned")
                    .live
                    .get(&id)
                    .is_some_and(|item| item.actions.iter().any(|(key, _)| key == &action_id));
                if !exists {
                    continue;
                }
                let emitter = SignalEmitter::new(&connection, OBJECT_PATH)?;
                NotificationInterface::action_invoked(&emitter, id, &action_id).await?;
                let _ = events
                    .send(NotificationEvent::ActionInvoked {
                        id,
                        action_id: action_id.clone(),
                    })
                    .await;
                NotificationInterface::notification_closed(&emitter, id, 2).await?;
                {
                    let mut state = state.lock().expect("notification state poisoned");
                    state.live.remove(&id);
                    state.expirations.remove(&id);
                }
                let _ = expiry_commands.send(ExpiryCommand::Cancel { id });
                let _ = events
                    .send(NotificationEvent::Closed { id, reason: 2 })
                    .await;
            }
            WorkerInput::Command(ServiceCommand::CloseFromSender { id, reply }) => {
                if !state
                    .lock()
                    .expect("notification state poisoned")
                    .live
                    .contains_key(&id)
                {
                    let _ = reply.send(CloseResult::UnknownId).await;
                    continue;
                }
                let signal_result = async {
                    let emitter = SignalEmitter::new(&connection, OBJECT_PATH)?;
                    NotificationInterface::notification_closed(&emitter, id, 3).await
                }
                .await;
                if let Err(error) = signal_result {
                    let _ = reply.send(CloseResult::Failed(error.to_string())).await;
                    continue;
                }
                {
                    let mut state = state.lock().expect("notification state poisoned");
                    state.live.remove(&id);
                    state.expirations.remove(&id);
                }
                let _ = expiry_commands.send(ExpiryCommand::Cancel { id });
                let _ = events
                    .send(NotificationEvent::Closed { id, reason: 3 })
                    .await;
                let _ = reply.send(CloseResult::Closed).await;
            }
            WorkerInput::Command(ServiceCommand::Expire { id, generation }) => {
                let expired = {
                    let mut state = state.lock().expect("notification state poisoned");
                    expire_notification_if_current(&mut state, id, generation, Instant::now())
                };
                if !expired {
                    continue;
                }
                // Invalidate the ID in the live store before publishing the
                // protocol signal, as required by NotificationClosed semantics.
                let emitter = SignalEmitter::new(&connection, OBJECT_PATH)?;
                NotificationInterface::notification_closed(&emitter, id, 1).await?;
                let _ = events
                    .send(NotificationEvent::Closed { id, reason: 1 })
                    .await;
            }
        }
    }
    Ok(())
}

struct NotificationInterface {
    state: Arc<Mutex<ServiceState>>,
    events: Sender<NotificationEvent>,
    commands: Sender<ServiceCommand>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationInterface {
    fn get_capabilities(&self) -> Vec<String> {
        vec!["actions".into(), "body".into()]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "iNtoo GPUI prototype".into(),
            "iNtoo".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<u32> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v).ok())
            .map(|value| match value {
                0 => NotificationUrgency::Low,
                2 => NotificationUrgency::Critical,
                _ => NotificationUrgency::Normal,
            })
            .unwrap_or_default();
        let image = parse_notification_image(&hints);
        let action_pairs = actions
            .chunks_exact(2)
            .map(|pair| (pair[0].clone(), pair[1].clone()))
            .collect();
        let (reply, response) = async_channel::bounded(1);
        self.commands
            .send(ServiceCommand::Notify {
                app_name: app_name.into(),
                replaces_id,
                app_icon: app_icon.into(),
                summary: summary.into(),
                body: body.into(),
                actions: action_pairs,
                urgency,
                expire_timeout_ms: expire_timeout,
                image,
                sender,
                reply,
            })
            .await
            .map_err(|_| zbus::fdo::Error::Failed("Notification worker stopped".into()))?;
        response
            .recv()
            .await
            .map_err(|_| zbus::fdo::Error::Failed("Notification worker stopped".into()))?
            .map_err(zbus::fdo::Error::Failed)
    }

    async fn close_notification(&self, id: u32) -> zbus::fdo::Result<()> {
        let (reply, response) = async_channel::bounded(1);
        self.commands
            .send(ServiceCommand::CloseFromSender { id, reply })
            .await
            .map_err(|_| zbus::fdo::Error::Failed("Notification worker stopped".into()))?;
        match response.recv().await {
            Ok(CloseResult::Closed) => Ok(()),
            Ok(CloseResult::UnknownId) => Err(zbus::fdo::Error::InvalidArgs(format!(
                "Unknown notification ID {id}"
            ))),
            Ok(CloseResult::Failed(error)) => Err(zbus::fdo::Error::Failed(error)),
            Err(_) => Err(zbus::fdo::Error::Failed(
                "Notification worker stopped".into(),
            )),
        }
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

fn allocate_notification_id(state: &mut ServiceState) -> Option<u32> {
    // Among live.len() + 1 nonzero candidates, at least one must be free.
    let candidates = (state.live.len() as u64 + 1).min(u32::MAX as u64);
    for _ in 0..candidates {
        let candidate = state.next_id.max(1);
        state.next_id = candidate.wrapping_add(1).max(1);
        if !state.live.contains_key(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn effective_expiration_timeout_ms(
    requested_timeout_ms: i32,
    urgency: NotificationUrgency,
) -> Option<u64> {
    match requested_timeout_ms {
        timeout if timeout > 0 => Some(timeout as u64),
        0 => None,
        // Match the current Waffle/QML fallback values in
        // Notifications.qml::_timeoutForNotification. The interface has no
        // runtime config input yet, so these are defaults, not saved settings.
        _ => match urgency {
            NotificationUrgency::Low => Some(5_000),
            NotificationUrgency::Normal => Some(7_000),
            NotificationUrgency::Critical => Some(30_000),
        },
    }
}

fn cancel_notification_timeout(state: &mut ServiceState, id: u32) -> bool {
    state.expirations.remove(&id).is_some()
}

fn expire_notification_if_current(
    state: &mut ServiceState,
    id: u32,
    generation: u64,
    now: Instant,
) -> bool {
    let matches_current =
        state
            .expirations
            .get(&id)
            .is_some_and(|(deadline, current_generation)| {
                *current_generation == generation && *deadline <= now
            });
    if !matches_current {
        return false;
    }

    state.expirations.remove(&id);
    state.live.remove(&id).is_some()
}

fn run_expiry_scheduler(
    commands: mpsc::Receiver<ExpiryCommand>,
    service_commands: Sender<ServiceCommand>,
) {
    let mut deadlines = BTreeMap::<(Instant, u32), u64>::new();
    let mut by_id = HashMap::<u32, (Instant, u64)>::new();

    loop {
        let next_deadline = deadlines
            .first_key_value()
            .map(|((deadline, _), _)| *deadline);
        let command = match next_deadline {
            Some(deadline) => {
                commands.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            }
            None => commands
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
        };

        match command {
            Ok(ExpiryCommand::Schedule {
                id,
                generation,
                deadline,
            }) => {
                if let Some((old_deadline, _)) = by_id.remove(&id) {
                    deadlines.remove(&(old_deadline, id));
                }
                deadlines.insert((deadline, id), generation);
                by_id.insert(id, (deadline, generation));
            }
            Ok(ExpiryCommand::Cancel { id }) => {
                if let Some((deadline, _)) = by_id.remove(&id) {
                    deadlines.remove(&(deadline, id));
                }
            }
            Ok(ExpiryCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                let due = deadlines
                    .range(..=(now, u32::MAX))
                    .map(|(key, generation)| (*key, *generation))
                    .collect::<Vec<_>>();
                for ((deadline, id), generation) in due {
                    deadlines.remove(&(deadline, id));
                    if by_id.get(&id) == Some(&(deadline, generation)) {
                        by_id.remove(&id);
                        if service_commands
                            .send_blocking(ServiceCommand::Expire { id, generation })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live_notification(id: u32) -> LiveNotification {
        LiveNotification {
            id,
            sender: ":1.42".into(),
            app_name: "Test app".into(),
            app_icon: String::new(),
            image: None,
            summary: "Test".into(),
            body: "Body".into(),
            actions: Vec::new(),
            urgency: NotificationUrgency::Normal,
            expire_timeout_ms: 1_000,
        }
    }

    #[test]
    fn canceled_timeout_rejects_queued_expiration_generation() {
        let now = Instant::now();
        let mut state = ServiceState {
            service: Some(NotificationServiceStatus::OwnerReady),
            next_id: 2,
            live: HashMap::from([(1, live_notification(1))]),
            expirations: HashMap::from([(1, (now - Duration::from_millis(1), 7))]),
            next_expiration_generation: 8,
        };

        assert!(cancel_notification_timeout(&mut state, 1));
        assert!(!cancel_notification_timeout(&mut state, 1));
        assert!(!expire_notification_if_current(&mut state, 1, 7, now));
        assert!(state.live.contains_key(&1));
    }

    #[test]
    fn stale_expiration_generation_cannot_remove_replacement_timeout() {
        let now = Instant::now();
        let mut state = ServiceState {
            service: Some(NotificationServiceStatus::OwnerReady),
            next_id: 2,
            live: HashMap::from([(1, live_notification(1))]),
            expirations: HashMap::from([(1, (now - Duration::from_millis(1), 8))]),
            next_expiration_generation: 9,
        };

        assert!(!expire_notification_if_current(&mut state, 1, 7, now));
        assert!(state.live.contains_key(&1));
        assert_eq!(
            state.expirations.get(&1).map(|(_, generation)| *generation),
            Some(8)
        );
    }

    #[test]
    fn valid_rgba_image_data_is_normalized_to_png() {
        let png = encode_notification_image_data(1, 1, 4, true, 8, 4, &[12, 34, 56, 78])
            .expect("valid image payload is encoded");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png.len() <= MAX_NOTIFICATION_IMAGE_PNG_BYTES);
    }

    #[test]
    fn standard_dbus_image_data_structure_is_parsed() {
        let structure = Structure::try_from((
            1_i32,
            1_i32,
            4_i32,
            true,
            8_i32,
            4_i32,
            vec![12_u8, 34, 56, 78],
        ))
        .expect("valid D-Bus image tuple");
        let value = OwnedValue::try_from(structure).expect("owned image-data hint");
        let hints = HashMap::from([("image-data".to_owned(), value)]);
        assert!(matches!(
            parse_notification_image(&hints),
            Some(NotificationImage::Png(bytes)) if bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        ));
    }

    #[test]
    fn malformed_and_oversized_image_payloads_fall_back_cleanly() {
        assert!(encode_notification_image_data(1, 1, 3, true, 8, 4, &[1, 2, 3]).is_none());
        assert!(encode_notification_image_data(1, 1, 4, true, 16, 4, &[1, 2, 3, 4]).is_none());
        assert!(encode_notification_image_data(5000, 1, 20000, false, 8, 3, &[]).is_none());
        assert!(encode_notification_image_data(1, 1, 3, false, 8, 3, &[1, 2, 3, 4]).is_none());
        assert!(encode_notification_image_data(2048, 2048, 8192, true, 8, 4, &[]).is_none());
        assert!(validate_image_resource("javascript:alert(1)".to_owned()).is_none());
        assert!(validate_image_resource("https://example.invalid/art.png".to_owned()).is_none());
        assert!(validate_image_resource("http://127.0.0.1:8080/art.png".to_owned()).is_none());
        assert_eq!(
            validate_image_resource("/tmp/notification.png".to_owned()),
            Some("/tmp/notification.png".to_owned())
        );
    }

    #[test]
    fn malformed_image_hint_does_not_hide_valid_path_hint() {
        let mut hints = HashMap::new();
        hints.insert("image-data".into(), OwnedValue::from(42_u32));
        hints.insert(
            "image-path".into(),
            OwnedValue::try_from(zbus::zvariant::Value::from(String::from(
                "file:///tmp/notification.png",
            )))
            .expect("owned image-path hint"),
        );
        assert_eq!(
            parse_notification_image(&hints),
            Some(NotificationImage::Resource(
                "file:///tmp/notification.png".into()
            ))
        );
    }

    #[test]
    fn replacing_live_notification_replaces_its_artwork() {
        let old = live_notification(7);
        let fresh_png: Arc<[u8]> = Arc::from(
            encode_notification_image_data(1, 1, 4, true, 8, 4, &[1, 2, 3, 255])
                .expect("replacement art encodes"),
        );
        let replacement = LiveNotification {
            image: Some(NotificationImage::Png(fresh_png.clone())),
            ..old
        };
        let mut state = ServiceState::default();
        state.live.insert(7, live_notification(7));
        state.live.insert(7, replacement.clone());
        assert_eq!(
            state.live.get(&7).and_then(|item| item.image.clone()),
            replacement.image
        );
        assert_eq!(state.live.get(&7).map(|item| item.id), Some(7));
    }
}
