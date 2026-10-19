//! Read-only PipeWire control-plane spike.
//!
//! All PipeWire proxies and listeners are confined to one dedicated thread.
//! Consumers receive owned, revisioned snapshots over an async channel. This
//! module deliberately exposes no volume, mute, or routing write operation.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

use async_channel::{Receiver, Sender};
use pipewire::{
    context::ContextRc,
    main_loop::MainLoopRc,
    node::{Node, NodeListener},
    registry::Listener as RegistryListener,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDirection {
    Output,
    Input,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioNodeKind {
    OutputDevice,
    InputDevice,
    OutputStream,
    InputStream,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioNodeSnapshot {
    /// PipeWire registry global ID. It is ephemeral and invalid after removal.
    pub global_id: u32,
    pub kind: AudioNodeKind,
    pub media_class: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub application_name: Option<String>,
    pub application_id: Option<String>,
    pub process_binary: Option<String>,
    /// SPA linear volume. The service must not assume channel values are equal.
    pub volume: Option<f32>,
    pub muted: Option<bool>,
    pub channel_volumes: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioSnapshot {
    pub revision: u64,
    pub initial_registry_complete: bool,
    pub nodes: Vec<AudioNodeSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioProviderEvent {
    Snapshot(AudioSnapshot),
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderStatus {
    Connecting,
    Ready,
    Unavailable,
}

enum LoopControl {
    Stop,
}

static PIPEWIRE_WORKER_ACTIVE: AtomicBool = AtomicBool::new(false);

struct PipeWireWorkerLease;

impl Drop for PipeWireWorkerLease {
    fn drop(&mut self) {
        PIPEWIRE_WORKER_ACTIVE.store(false, Ordering::Release);
    }
}

/// Owns the PipeWire worker lifecycle and its event receiver.
///
/// Dropping the provider asks the PipeWire loop to stop. A caller must treat
/// `Connecting` as not-ready and only expose data after receiving a `Snapshot`
/// with `initial_registry_complete == true`.
pub struct PipeWireAudioProvider {
    status: Arc<std::sync::atomic::AtomicU8>,
    controls: Option<pipewire::channel::Sender<LoopControl>>,
    worker: Option<JoinHandle<()>>,
    event_sender: Sender<AudioProviderEvent>,
    events: Receiver<AudioProviderEvent>,
}

impl PipeWireAudioProvider {
    pub fn start() -> Self {
        let status = Arc::new(std::sync::atomic::AtomicU8::new(0));
        let worker_status = status.clone();
        let (event_sender, events) = async_channel::unbounded();
        let (controls, control_receiver) = pipewire::channel::channel::<LoopControl>();
        if PIPEWIRE_WORKER_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            status.store(2, Ordering::Release);
            let _ = event_sender.send_blocking(AudioProviderEvent::Unavailable(
                "A PipeWire provider is already active or stopping".into(),
            ));
            return Self {
                status,
                controls: None,
                worker: None,
                event_sender,
                events,
            };
        }
        let worker_lease = PipeWireWorkerLease;
        let worker_events = event_sender.clone();
        let worker = thread::Builder::new()
            .name("intoo-pipewire-registry".into())
            .spawn(move || {
                run_pipewire(worker_status, worker_events, control_receiver, worker_lease)
            });

        match worker {
            Ok(worker) => Self {
                status,
                controls: Some(controls),
                worker: Some(worker),
                event_sender,
                events,
            },
            Err(error) => {
                PIPEWIRE_WORKER_ACTIVE.store(false, Ordering::Release);
                let _ = event_sender.send_blocking(AudioProviderEvent::Unavailable(format!(
                    "Could not start PipeWire worker: {error}"
                )));
                status.store(2, std::sync::atomic::Ordering::Release);
                Self {
                    status,
                    controls: None,
                    worker: None,
                    event_sender,
                    events,
                }
            }
        }
    }

    pub fn status(&self) -> ProviderStatus {
        match self.status.load(std::sync::atomic::Ordering::Acquire) {
            1 => ProviderStatus::Ready,
            2 => ProviderStatus::Unavailable,
            _ => ProviderStatus::Connecting,
        }
    }

    pub fn events(&self) -> Receiver<AudioProviderEvent> {
        self.events.clone()
    }

    /// Stop the worker and hand its join to a reaper thread.
    ///
    /// This returns without waiting. Reaper spawn/join failures update provider
    /// status and are sent through the provider event channel. `Drop` uses the
    /// same non-blocking shutdown path.
    pub fn shutdown_background(mut self) -> Result<(), String> {
        let send_failed = self
            .controls
            .take()
            .is_some_and(|controls| controls.send(LoopControl::Stop).is_err());
        let reaper_result = self.worker.take().map_or(Ok(()), |worker| {
            spawn_reaper(worker, self.status.clone(), self.event_sender.clone())
        });
        match (send_failed, reaper_result) {
            (_, Err(error)) => Err(error),
            (true, Ok(())) => Err("PipeWire worker already stopped".to_owned()),
            (false, Ok(())) => Ok(()),
        }
    }
}

impl Drop for PipeWireAudioProvider {
    fn drop(&mut self) {
        if let Some(controls) = self.controls.take() {
            let _ = controls.send(LoopControl::Stop);
        }
        if let Some(worker) = self.worker.take() {
            let _ = spawn_reaper(worker, self.status.clone(), self.event_sender.clone());
        }
    }
}

fn spawn_reaper(
    worker: JoinHandle<()>,
    status: Arc<std::sync::atomic::AtomicU8>,
    events: Sender<AudioProviderEvent>,
) -> Result<(), String> {
    let reaper_status = status.clone();
    let reaper_events = events.clone();
    thread::Builder::new()
        .name("intoo-pipewire-reaper".into())
        .spawn(move || {
            if worker.join().is_err() {
                reaper_status.store(2, std::sync::atomic::Ordering::Release);
                let _ = reaper_events.send_blocking(AudioProviderEvent::Unavailable(
                    "PipeWire worker panicked".into(),
                ));
            }
        })
        .map(|_| ())
        .map_err(|error| {
            status.store(2, std::sync::atomic::Ordering::Release);
            let message = format!(
                "Could not start PipeWire reaper; worker detached after stop request: {error}"
            );
            let _ = events.send_blocking(AudioProviderEvent::Unavailable(message.clone()));
            message
        })
}

struct NodeRuntime {
    _node: Node,
    _listener: NodeListener,
}

fn run_pipewire(
    status: Arc<std::sync::atomic::AtomicU8>,
    events: Sender<AudioProviderEvent>,
    controls: pipewire::channel::Receiver<LoopControl>,
    _worker_lease: PipeWireWorkerLease,
) {
    // The crate's init() itself is process-once. It must not be paired with
    // deinit here: deinit is safe only once, after every PipeWire thread and
    // resource in the process has stopped.
    pipewire::init();
    let result = (|| -> Result<(), String> {
        let main_loop = MainLoopRc::new(None).map_err(|e| e.to_string())?;
        let context = ContextRc::new(&main_loop, None).map_err(|e| e.to_string())?;
        let core = context.connect_rc(None).map_err(|e| e.to_string())?;
        let registry = core.get_registry_rc().map_err(|e| e.to_string())?;
        let state = Rc::new(RefCell::new(BTreeMap::<u32, AudioNodeSnapshot>::new()));
        let runtimes = Rc::new(RefCell::new(BTreeMap::<u32, NodeRuntime>::new()));
        let revision = Rc::new(RefCell::new(0_u64));
        let initial_complete = Rc::new(RefCell::new(false));

        let loop_for_control = main_loop.clone();
        let _control_receiver = controls.attach(main_loop.loop_(), move |control| match control {
            LoopControl::Stop => loop_for_control.quit(),
        });

        let emit = {
            let state = state.clone();
            let revision = revision.clone();
            let initial_complete = initial_complete.clone();
            let events = events.clone();
            let status = status.clone();
            move || {
                let mut next_revision = revision.borrow_mut();
                *next_revision = next_revision.wrapping_add(1);
                let snapshot = AudioSnapshot {
                    revision: *next_revision,
                    initial_registry_complete: *initial_complete.borrow(),
                    nodes: state.borrow().values().cloned().collect(),
                };
                if snapshot.initial_registry_complete {
                    status.store(1, std::sync::atomic::Ordering::Release);
                }
                let _ = events.send_blocking(AudioProviderEvent::Snapshot(snapshot));
            }
        };

        let registry_for_globals = registry.clone();
        let state_for_globals = state.clone();
        let runtimes_for_globals = runtimes.clone();
        let emit_for_globals = emit.clone();
        let registry_listener: RegistryListener = registry
            .add_listener_local()
            .global(move |global| {
                if global.type_.to_str() != "PipeWire:Interface:Node" {
                    return;
                }
                let Some(props) = global.props.as_ref() else {
                    return;
                };
                let node_snapshot = snapshot_from_global(global.id, props);
                state_for_globals
                    .borrow_mut()
                    .insert(global.id, node_snapshot);

                match registry_for_globals.bind::<Node, _>(global) {
                    Ok(node) => {
                        let state_for_info = state_for_globals.clone();
                        let emit_for_info = emit_for_globals.clone();
                        let node_id = global.id;
                        let listener = node
                            .add_listener_local()
                            .info(move |info| {
                                if let Some(current) = state_for_info.borrow_mut().get_mut(&node_id)
                                {
                                    if let Some(props) = info.props() {
                                        current.name = dict_value(props, "node.name");
                                        current.description = dict_value(props, "node.description");
                                        current.application_name =
                                            dict_value(props, "application.name");
                                        current.application_id =
                                            dict_value(props, "application.id");
                                        current.process_binary =
                                            dict_value(props, "application.process.binary");
                                        current.media_class = dict_value(props, "media.class");
                                        current.kind = classify(current.media_class.as_deref());
                                    }
                                }
                                emit_for_info();
                            })
                            .param({
                                let state_for_param = state_for_globals.clone();
                                let emit_for_param = emit_for_globals.clone();
                                move |_, param_type, _, _, pod| {
                                    if param_type != pipewire::spa::param::ParamType::Props {
                                        return;
                                    }
                                    let Some(pod) = pod else { return };
                                    if let Some(current) =
                                        state_for_param.borrow_mut().get_mut(&node_id)
                                    {
                                        apply_props(current, pod.as_bytes());
                                    }
                                    emit_for_param();
                                }
                            })
                            .register();
                        // X permission is required for both enumeration and subscription.
                        node.enum_params(
                            0,
                            Some(pipewire::spa::param::ParamType::Props),
                            0,
                            u32::MAX,
                        );
                        node.subscribe_params(&[pipewire::spa::param::ParamType::Props]);
                        runtimes_for_globals.borrow_mut().insert(
                            global.id,
                            NodeRuntime {
                                _node: node,
                                _listener: listener,
                            },
                        );
                    }
                    Err(_) => {
                        // Retain registry metadata but keep Props unknown if the
                        // session manager denies binding this node.
                    }
                }
                emit_for_globals();
            })
            .global_remove({
                let state = state.clone();
                let runtimes = runtimes.clone();
                let emit = emit.clone();
                move |id| {
                    runtimes.borrow_mut().remove(&id);
                    state.borrow_mut().remove(&id);
                    emit();
                }
            })
            .register();

        let sync_sequence = core.sync(0).map_err(|e| e.to_string())?;
        let state_for_done = state.clone();
        let revision_for_done = revision.clone();
        let initial_for_done = initial_complete.clone();
        let events_for_done = events.clone();
        let status_for_done = status.clone();
        let events_for_error = events.clone();
        let status_for_error = status.clone();
        let loop_for_error = main_loop.clone();
        let _core_listener = core
            .add_listener_local()
            .done(move |id, seq| {
                if id == pipewire::core::PW_ID_CORE && seq.seq() == sync_sequence.seq() {
                    *initial_for_done.borrow_mut() = true;
                    status_for_done.store(1, std::sync::atomic::Ordering::Release);
                    let mut next_revision = revision_for_done.borrow_mut();
                    *next_revision = next_revision.wrapping_add(1);
                    let snapshot = AudioSnapshot {
                        revision: *next_revision,
                        initial_registry_complete: true,
                        nodes: state_for_done.borrow().values().cloned().collect(),
                    };
                    let _ = events_for_done.send_blocking(AudioProviderEvent::Snapshot(snapshot));
                }
            })
            .error(move |id, _, result, message| {
                if id == pipewire::core::PW_ID_CORE {
                    status_for_error.store(2, std::sync::atomic::Ordering::Release);
                    let _ = events_for_error.send_blocking(AudioProviderEvent::Unavailable(
                        format!("PipeWire core error {result}: {message}"),
                    ));
                    loop_for_error.quit();
                }
            })
            .register();

        // Keep all listeners and object owners alive for the duration of run.
        let _keep_alive = (registry_listener, runtimes, registry);
        main_loop.run();
        Ok(())
    })();

    if let Err(error) = result {
        status.store(2, std::sync::atomic::Ordering::Release);
        let _ = events.send_blocking(AudioProviderEvent::Unavailable(error));
    }
}

fn snapshot_from_global(
    id: u32,
    props: &impl AsRef<pipewire::spa::utils::dict::DictRef>,
) -> AudioNodeSnapshot {
    let props = props.as_ref();
    let media_class = dict_value(props, "media.class");
    AudioNodeSnapshot {
        global_id: id,
        kind: classify(media_class.as_deref()),
        media_class,
        name: dict_value(props, "node.name"),
        description: dict_value(props, "node.description"),
        application_name: dict_value(props, "application.name"),
        application_id: dict_value(props, "application.id"),
        process_binary: dict_value(props, "application.process.binary"),
        volume: None,
        muted: None,
        channel_volumes: Vec::new(),
    }
}

fn dict_value(props: &pipewire::spa::utils::dict::DictRef, key: &str) -> Option<String> {
    props.get(key).map(str::to_owned)
}

fn classify(media_class: Option<&str>) -> AudioNodeKind {
    match media_class.unwrap_or_default() {
        "Audio/Sink" => AudioNodeKind::OutputDevice,
        "Audio/Source" => AudioNodeKind::InputDevice,
        "Stream/Output/Audio" => AudioNodeKind::OutputStream,
        "Stream/Input/Audio" => AudioNodeKind::InputStream,
        _ => AudioNodeKind::Other,
    }
}

fn apply_props(snapshot: &mut AudioNodeSnapshot, bytes: &[u8]) {
    use libspa::pod::{Value, deserialize::PodDeserializer};

    let Ok((_, Value::Object(object))) = PodDeserializer::deserialize_any_from(bytes) else {
        return;
    };
    let volume_key = libspa::sys::SPA_PROP_volume;
    let mute_key = libspa::sys::SPA_PROP_mute;
    let channel_volumes_key = libspa::sys::SPA_PROP_channelVolumes;
    for property in object.properties {
        match property.key {
            key if key == volume_key => {
                snapshot.volume = match property.value {
                    Value::Float(value) => Some(value),
                    Value::Double(value) => Some(value as f32),
                    _ => snapshot.volume,
                };
            }
            key if key == mute_key => {
                snapshot.muted = match property.value {
                    Value::Bool(value) => Some(value),
                    _ => snapshot.muted,
                };
            }
            key if key == channel_volumes_key => {
                if let Value::ValueArray(libspa::pod::ValueArray::Float(values)) = property.value {
                    snapshot.channel_volumes = values;
                }
            }
            _ => {}
        }
    }
}
