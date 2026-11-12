//! Minimal owned-buffer ABI. Host must pass only live allocations from this module.
//! JSON is used for the prototype; no per-node boundary calls.
#[no_mangle]
pub extern "C" fn lw_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
}
/// # Safety
/// `ptr,len` must describe a live buffer returned by lw_alloc or lw_run, freed once.
#[no_mangle]
pub unsafe extern "C" fn lw_free(ptr: *mut u8, len: usize) {
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
}
// Host import: receives one progressive publication as UTF-8 JSON while `lw_run` is still executing. The host copies
// the bytes before it returns; the buffer belongs to this module and is released after the call.
// The import module is named explicitly, so the linker resolves the symbol as a host import instead of reporting it as
// undefined (the CI linker does this for an implicit module).
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn lw_publish(ptr: *const u8, len: usize);
}

/// Hands one publication to the host. A no-op on native targets, where there is no host to receive it.
fn publish_to_host(message: &str) {
    #[cfg(target_arch = "wasm32")]
    // SAFETY: the buffer is live for the duration of the call, and the host copies it before returning.
    unsafe {
        lw_publish(message.as_ptr(), message.len());
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = message;
}

/// # Safety
/// Input must be a valid live allocation of `len` bytes. The caller retains ownership.
/// Returns output pointer (low 32 bits) and length (high 32 bits). For `refineProgressive`, every publication is
/// handed to the host through `lw_publish` before this function returns.
#[no_mangle]
pub unsafe extern "C" fn lw_run(ptr: *const u8, len: usize) -> u64 {
    let bytes = std::slice::from_raw_parts(ptr, len);
    let result = match std::str::from_utf8(bytes) {
        Ok(s) => arqlet_runtime::dispatch_streaming(s, &mut publish_to_host),
        Err(_) => "{\"ok\":false,\"error\":\"Invalid UTF-8\"}".into(),
    };
    let out = result.into_bytes().into_boxed_slice();
    let size = out.len() as u64;
    let address = Box::into_raw(out) as *mut u8 as u64;
    (size << 32) | (address & 0xffffffff)
}

// Provisional (ADR 0019, step 3): sessions whose snapshot stays in this module. The host opens one, sends requests without
// the snapshot (see `arqlet_runtime::engine_session`), and closes it. WASM runs on one thread, so a thread-local store is
// the whole store; handles are not reused while the module lives.
thread_local! {
    static SESSIONS: std::cell::RefCell<(u32, std::collections::BTreeMap<u32, arqlet_runtime::engine_session::EngineSession>)> =
        const { std::cell::RefCell::new((0, std::collections::BTreeMap::new())) };
}

/// Opens an empty session and returns its handle (never 0).
#[no_mangle]
pub extern "C" fn lw_session_open() -> u32 {
    SESSIONS.with(|s| {
        let mut s = s.borrow_mut();
        s.0 += 1;
        let id = s.0;
        s.1.insert(id, Default::default());
        id
    })
}

/// Closes a session and frees its snapshot. Closing an unknown handle does nothing.
#[no_mangle]
pub extern "C" fn lw_session_close(id: u32) {
    SESSIONS.with(|s| {
        s.borrow_mut().1.remove(&id);
    });
}

/// # Safety
/// As for `lw_run`: input must be a valid live allocation of `len` bytes, which the caller keeps; the answer is returned
/// as pointer (low 32 bits) and length (high 32 bits) and freed by the caller with `lw_free`. An unknown handle answers
/// with an error.
#[no_mangle]
pub unsafe extern "C" fn lw_session_run(id: u32, ptr: *const u8, len: usize) -> u64 {
    let bytes = std::slice::from_raw_parts(ptr, len);
    let result = match std::str::from_utf8(bytes) {
        Ok(text) => SESSIONS.with(|s| match s.borrow_mut().1.get_mut(&id) {
            Some(session) => session.run(text),
            None => "{\"ok\":false,\"error\":\"Unknown session\"}".into(),
        }),
        Err(_) => "{\"ok\":false,\"error\":\"Invalid UTF-8\"}".into(),
    };
    let out = result.into_bytes().into_boxed_slice();
    let size = out.len() as u64;
    let address = Box::into_raw(out) as *mut u8 as u64;
    (size << 32) | (address & 0xffffffff)
}
