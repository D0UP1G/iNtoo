use gpui::WindowKind;

fn main() {
    let _compile_only_kind = WindowKind::SessionLock;
    let _backend_factory = gpui_platform::current_platform;
}

#[cfg(test)]
#[path = "../vendor/zed/crates/gpui_linux/src/linux/wayland/session_lock.rs"]
mod session_lock;
