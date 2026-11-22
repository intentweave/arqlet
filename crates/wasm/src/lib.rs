//! Minimal owned-buffer ABI. Host must pass only live allocations from this module.
//! JSON is used for the prototype; no per-node boundary calls.
#[no_mangle]
pub extern "C" fn lw_alloc(len: usize) -> *mut u8 {
    // Every request starts with an allocation: the host's clock is installed before the first one runs.
    install_clock();
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
    /// The host's clock in ms (`performance.now()`), for phase timings only (`timings: true` on a request).
    fn lw_now() -> f64;
}

/// Installs the host's clock (provisional, phase timings). The layout never reads it; only reported times do.
fn install_clock() {
    #[cfg(target_arch = "wasm32")]
    arqlet_runtime::clock::install(|| {
        // SAFETY: the import takes no arguments and returns a number.
        unsafe { lw_now() }
    });
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
            Some(session) => with_heap(text, session.run(text)),
            None => "{\"ok\":false,\"error\":\"Unknown session\"}".into(),
        }),
        Err(_) => "{\"ok\":false,\"error\":\"Invalid UTF-8\"}".into(),
    };
    let out = result.into_bytes().into_boxed_slice();
    let size = out.len() as u64;
    let address = Box::into_raw(out) as *mut u8 as u64;
    (size << 32) | (address & 0xffffffff)
}

/// Provisional (ADR 0019, step 4): the memory of one column of the session's last `columns` request, as pointer (low 32
/// bits) and length in bytes (high 32 bits). `index` is the column's position in that answer's `columns` list. The
/// memory belongs to the session: the host must not free it, reads it as a typed-array view of the module's memory and
/// copies it if it needs the numbers after the next request to this session (any change drops or moves them, and a
/// growth of the module's memory detaches views: take the view again from `memory.buffer`). Answers 0 for an unknown
/// session, a session without columns, or an unknown index.
#[no_mangle]
pub extern "C" fn lw_session_column(id: u32, index: u32) -> u64 {
    SESSIONS.with(|s| {
        let s = s.borrow();
        let Some(columns) = s.1.get(&id).and_then(|session| session.columns()) else {
            return 0;
        };
        let Some((_, column)) = columns.columns().into_iter().nth(index as usize) else {
            return 0;
        };
        let (ptr, _) = column.raw();
        let bytes = column.bytes() as u64;
        (bytes << 32) | (ptr as u64 & 0xffffffff)
    })
}

/// Provisional (ADR 0019, step 4): as `lw_session_column`, for the session's last `overview` request with `columns: true`
/// (column `index` of the `columns` list in that answer's `overview`). Valid until the session's next overview request.
#[no_mangle]
pub extern "C" fn lw_session_overview_column(id: u32, index: u32) -> u64 {
    SESSIONS.with(|s| {
        let s = s.borrow();
        let Some(column) =
            s.1.get(&id)
                .and_then(|session| session.overview_columns())
                .and_then(|columns| columns.column(index as usize))
        else {
            return 0;
        };
        let (ptr, _) = column.raw();
        ((column.bytes() as u64) << 32) | (ptr as u64 & 0xffffffff)
    })
}

/// Provisional (ADR 0023, R7): the heap bytes the module has allocated and not freed, counted by a wrapper around the
/// system allocator (one addition per allocation; it changes no result).
struct Counting;
static LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let p = std::alloc::System.alloc(layout);
        if !p.is_null() {
            LIVE.fetch_add(layout.size(), std::sync::atomic::Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: std::alloc::Layout) {
        std::alloc::System.dealloc(p, layout);
        LIVE.fetch_sub(layout.size(), std::sync::atomic::Ordering::Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        let q = std::alloc::System.realloc(p, layout, size);
        if !q.is_null() {
            if size >= layout.size() {
                LIVE.fetch_add(size - layout.size(), std::sync::atomic::Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - size, std::sync::atomic::Ordering::Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The answer to a session's `memory` or `release` with what only the module knows: the live heap of the whole module
/// (every session and the answer being written) and the size of its linear memory, which WebAssembly never gives back.
fn with_heap(request: &str, answer: String) -> String {
    if !(request.contains("\"memory\"") || request.contains("\"release\""))
        || !answer.contains("\"estimated\":true")
        || !answer.ends_with('}')
    {
        return answer;
    }
    let live = LIVE.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(target_arch = "wasm32")]
    let linear = format!(
        ",\"linearMemoryBytes\":{}",
        core::arch::wasm32::memory_size(0) as u64 * 65_536
    );
    #[cfg(not(target_arch = "wasm32"))]
    let linear = String::new();
    let mut out = answer;
    out.pop();
    out.push_str(&format!(",\"heapBytes\":{live}{linear}}}"));
    out
}
