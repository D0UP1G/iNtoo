use intoo_session_lock_probe::{Configure, LockFsm, LockPhase, LockRequest};
use memmap2::MmapMut;
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::BufRead,
    os::fd::{AsFd, AsRawFd},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface},
};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1,
};
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

struct SurfaceEntry {
    surface: wl_surface::WlSurface,
    lock_surface: ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
    latest_configure: Option<Configure>,
}
struct Backing {
    _buffer: wl_buffer::WlBuffer,
    _pool: wl_shm_pool::WlShmPool,
    _mapping: MmapMut,
    _file: File,
}
struct State {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    manager: Option<ext_session_lock_manager_v1::ExtSessionLockManagerV1>,
    lock: Option<ext_session_lock_v1::ExtSessionLockV1>,
    outputs: HashMap<u32, wl_output::WlOutput>,
    surfaces: HashMap<u32, SurfaceEntry>,
    buffers: Vec<Backing>,
    fsm: LockFsm,
}
impl State {
    fn new() -> Self {
        Self {
            compositor: None,
            shm: None,
            manager: None,
            lock: None,
            outputs: HashMap::new(),
            surfaces: HashMap::new(),
            buffers: Vec::new(),
            fsm: LockFsm::default(),
        }
    }
    fn create_lock_surface(
        &mut self,
        id: u32,
        output: &wl_output::WlOutput,
        qh: &QueueHandle<Self>,
    ) -> Result<(), String> {
        if self.surfaces.contains_key(&id) {
            return Ok(());
        }
        let compositor = self
            .compositor
            .as_ref()
            .ok_or("wl_compositor unavailable")?;
        let lock = self.lock.as_ref().ok_or("session lock unavailable")?;
        // This wl_surface has never had a role, buffer, or commit.
        let surface = compositor.create_surface(qh, ());
        let lock_surface = lock.get_lock_surface(&surface, output, qh, id);
        self.surfaces.insert(
            id,
            SurfaceEntry {
                surface,
                lock_surface,
                latest_configure: None,
            },
        );
        eprintln!("output {id}: created pristine session-lock surface");
        Ok(())
    }
    fn add_output(
        &mut self,
        id: u32,
        version: u32,
        registry: &wl_registry::WlRegistry,
        qh: &QueueHandle<Self>,
    ) {
        if self.outputs.contains_key(&id) {
            return;
        }
        let output = registry.bind::<wl_output::WlOutput, _, _>(id, version.min(4), qh, ());
        self.outputs.insert(id, output.clone());
        if self.lock.is_some() {
            if let Err(e) = self.create_lock_surface(id, &output, qh) {
                eprintln!("cannot create lock surface: {e}");
            }
        }
    }
    fn remove_output(&mut self, id: u32) {
        if let Some(entry) = self.surfaces.remove(&id) {
            entry.lock_surface.destroy();
            entry.surface.destroy();
            eprintln!("output {id}: removed lock surface");
        }
        self.outputs.remove(&id);
    }
    fn create_shm_buffer(&mut self, c: Configure, qh: &QueueHandle<Self>) -> Result<usize, String> {
        let bytes = c.validate().map_err(str::to_owned)?;
        let shm = self.shm.as_ref().ok_or("wl_shm unavailable")?;
        let path = shm_temp_path();
        let mut opts = OpenOptions::new();
        opts.read(true).write(true).create_new(true).mode(0o600);
        let file = opts
            .open(&path)
            .map_err(|e| format!("create SHM backing {}: {e}", path.display()))?;
        std::fs::remove_file(&path).map_err(|e| format!("unlink SHM backing: {e}"))?;
        file.set_len(bytes as u64)
            .map_err(|e| format!("size SHM backing: {e}"))?;
        // SAFETY: file length is checked and bounded by Configure::validate.
        let mut mapping =
            unsafe { MmapMut::map_mut(&file) }.map_err(|e| format!("map SHM backing: {e}"))?;
        for p in mapping.chunks_exact_mut(4) {
            p.copy_from_slice(&[0x20, 0x20, 0x20, 0xff]);
        }
        let pool = shm.create_pool(file.as_fd(), bytes as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            c.width as i32,
            c.height as i32,
            (c.width as i32) * 4,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );
        let id = self.buffers.len();
        self.buffers.push(Backing {
            _buffer: buffer,
            _pool: pool,
            _mapping: mapping,
            _file: file,
        });
        Ok(id)
    }
    fn on_configure(
        &mut self,
        id: u32,
        c: Configure,
        qh: &QueueHandle<Self>,
    ) -> Result<(), String> {
        c.validate().map_err(str::to_owned)?;
        if self
            .surfaces
            .get(&id)
            .ok_or("configure for removed output")?
            .latest_configure
            .is_some_and(|last| last.serial == c.serial)
        {
            return Err("duplicate configure serial".into());
        }
        let buffer_id = self.create_shm_buffer(c, qh)?;
        if !c.accepts_buffer(c.width, c.height, false) {
            return Err("buffer does not match configure".into());
        }
        let entry = self
            .surfaces
            .get_mut(&id)
            .ok_or("configure for removed output")?;
        // Protocol ordering: ack serial before attaching/committing its exact-size opaque buffer.
        entry.lock_surface.ack_configure(c.serial);
        entry
            .surface
            .attach(Some(&self.buffers[buffer_id]._buffer), 0, 0);
        entry
            .surface
            .damage_buffer(0, 0, c.width as i32, c.height as i32);
        entry.surface.commit();
        entry.latest_configure = Some(c);
        eprintln!(
            "output {id}: acked configure {} and committed opaque {}x{} SHM buffer",
            c.serial, c.width, c.height
        );
        Ok(())
    }
    fn teardown_lock(&mut self, req: LockRequest) {
        if let Some(lock) = self.lock.take() {
            match req {
                LockRequest::Destroy => lock.destroy(),
                LockRequest::UnlockAndDestroy => lock.unlock_and_destroy(),
            }
        }
        for (_, entry) in self.surfaces.drain() {
            entry.lock_surface.destroy();
            entry.surface.destroy();
        }
    }
}
fn shm_temp_path() -> PathBuf {
    let dir = if std::path::Path::new("/dev/shm").is_dir() {
        "/dev/shm"
    } else {
        "/tmp"
    };
    PathBuf::from(dir).join(format!(
        "intoo-lock-probe-{}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ))
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        r: &wl_registry::WlRegistry,
        e: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        match e {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_compositor" => s.compositor = Some(r.bind(name, version.min(4), q, ())),
                "wl_shm" => s.shm = Some(r.bind(name, 1, q, ())),
                "wl_output" => s.add_output(name, version, r, q),
                "ext_session_lock_manager_v1" => s.manager = Some(r.bind(name, 1, q, ())),
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => s.remove_output(name),
            _ => {}
        }
    }
}
impl Dispatch<ext_session_lock_manager_v1::ExtSessionLockManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ext_session_lock_manager_v1::ExtSessionLockManagerV1,
        _: ext_session_lock_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ext_session_lock_v1::ExtSessionLockV1, ()> for State {
    fn event(
        s: &mut Self,
        _: &ext_session_lock_v1::ExtSessionLockV1,
        e: ext_session_lock_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match e {
            ext_session_lock_v1::Event::Locked => match s.fsm.locked() {
                Ok(()) => eprintln!("compositor confirmed session locked (all outputs protected)"),
                Err(e) => {
                    eprintln!("unexpected locked event: {e}");
                    s.teardown_lock(LockRequest::Destroy);
                }
            },
            ext_session_lock_v1::Event::Finished => match s.fsm.finished() {
                Ok(req) => {
                    eprintln!("compositor sent finished; responding with {req:?}");
                    s.teardown_lock(req);
                }
                Err(e) => eprintln!("unexpected finished event: {e}"),
            },
            _ => {}
        }
    }
}
impl Dispatch<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, u32> for State {
    fn event(
        s: &mut Self,
        _: &ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
        e: ext_session_lock_surface_v1::Event,
        id: &u32,
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = e
        {
            if let Err(e) = s.on_configure(
                *id,
                Configure {
                    serial,
                    width,
                    height,
                },
                q,
            ) {
                eprintln!("output {id}: configure failed closed: {e}");
            }
        }
    }
}
delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_output::WlOutput);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        help();
        return;
    }
    if args.as_slice() == ["--dry-run"] {
        println!(
            "dry-run: no Wayland connection or lock request\nWAYLAND_DISPLAY={}\nprotocol: ext-session-lock-v1 (wayland-protocols 0.32.13)\nlive test requires a dedicated disposable compositor",
            std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "<unset>".into())
        );
        return;
    }
    if args.as_slice() != ["--run"]
        || std::env::var("INTOO_SESSION_LOCK_PROBE_CONFIRM").as_deref()
            != Ok("I_UNDERSTAND_THIS_LOCKS_THE_SESSION")
    {
        eprintln!(
            "refusing to connect; use --dry-run, or on a disposable test compositor set INTOO_SESSION_LOCK_PROBE_CONFIRM=I_UNDERSTAND_THIS_LOCKS_THE_SESSION and pass --run"
        );
        std::process::exit(2);
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("WAYLAND_DISPLAY is unset; refusing live run");
        std::process::exit(2);
    }
    if let Err(e) = live_probe() {
        eprintln!("probe failed: {e}");
        std::process::exit(1);
    }
}
fn live_probe() -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("connect Wayland: {e}"))?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut state = State::new();
    queue
        .roundtrip(&mut state)
        .map_err(|e| format!("registry roundtrip: {e}"))?;
    if state.compositor.is_none() || state.shm.is_none() || state.manager.is_none() {
        return Err(format!(
            "required globals missing (compositor={}, shm={}, lock_manager={}); no lock/surface created",
            state.compositor.is_some(),
            state.shm.is_some(),
            state.manager.is_some()
        ));
    }
    if state.outputs.is_empty() {
        return Err("no wl_output globals; lock not requested".into());
    }
    state.fsm.request().map_err(str::to_owned)?;
    let lock = state.manager.as_ref().unwrap().lock(&qh, ());
    state.lock = Some(lock);
    let outputs: Vec<_> = state
        .outputs
        .iter()
        .map(|(id, output)| (*id, output.clone()))
        .collect();
    for (id, output) in outputs {
        state.create_lock_surface(id, &output, &qh)?;
    }
    conn.flush()
        .map_err(|e| format!("flush lock request: {e}"))?;
    eprintln!("lock requested; after locked, press Enter to unlock");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
        let _ = tx.send(());
    });
    let mut unlock_requested = false;
    while state.fsm.phase() != LockPhase::Denied && state.fsm.phase() != LockPhase::Released {
        queue
            .dispatch_pending(&mut state)
            .map_err(|e| format!("Wayland dispatch: {e}"))?;
        if rx.try_recv().is_ok() {
            unlock_requested = true;
        }
        // Latch an early Enter, but never unlock before the compositor confirms
        // that every output is protected with its locked frame.
        if unlock_requested && state.fsm.phase() == LockPhase::Locked {
            let request = state
                .fsm
                .unlock()
                .map_err(|e| format!("invalid unlock transition: {e}"))?;
            eprintln!("manual unlock requested");
            state.teardown_lock(request);
            continue;
        }
        conn.flush().map_err(|e| format!("flush Wayland: {e}"))?;
        let Some(read) = queue.prepare_read() else {
            continue;
        };
        let mut fd = libc::pollfd {
            fd: conn.as_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: fd points to one initialized pollfd and count is exactly one.
        let ready = unsafe { libc::poll(&mut fd, 1, 250) };
        if ready < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("poll Wayland: {e}"));
        }
        if ready > 0
            && fd.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0
        {
            read.read().map_err(|e| format!("read Wayland: {e}"))?;
        }
    }
    conn.flush().map_err(|e| format!("flush teardown: {e}"))?;
    if state.fsm.phase() == LockPhase::Released {
        queue
            .roundtrip(&mut state)
            .map_err(|e| format!("post-unlock sync: {e}"))?;
    }
    Ok(())
}
fn help() {
    println!(
        "intoo-session-lock-probe\n\n  --dry-run  Print diagnostics without connecting\n  --run      Request a real session lock (dedicated compositor only)\n  --help     Show help\n\n--run requires INTOO_SESSION_LOCK_PROBE_CONFIRM=I_UNDERSTAND_THIS_LOCKS_THE_SESSION"
    );
}
