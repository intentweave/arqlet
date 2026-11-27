//! Provisional (ADR 0030, ADR 0023 S1): the engine as a native Node.js module (Node-API through napi-rs). The same entry
//! points as the WASM adapter (`crates/wasm`): `dispatch` on JSON text, and sessions whose snapshot stays in the module
//! (open, run, close, the scene's and the overview's columns), with `memory` and `release` answered with the module's
//! live heap. The answers are the same bytes as the WASM engine's and the native binaries' (`tools/platform-parity.mjs
//! --node`).
//!
//! Every call that computes runs as an asynchronous task on libuv's thread pool (`napi_create_async_work`): the request
//! text is copied out of JavaScript on the main thread, the engine runs on a pool thread, and the answer text is handed
//! back on the main thread, so the event loop stays free during a layout. Sessions live in one
//! `arqlet_runtime::shared_sessions::SharedSessionStore` per `NativeEngine`: different sessions run in parallel, a session
//! runs one request at a time (its own lock), and the store's book (map, weights, least-recently-used order) is locked
//! only briefly, never during a computation. The JavaScript loader (`packages/node`) sends a session's next request only
//! after the answer to the previous one, so they also run in order.
//!
//! Only this crate touches Node; the engine's crates stay free of it (CLAUDE.md).
#![deny(clippy::all)]

use arqlet_runtime::shared_sessions::{Missing, SharedSessionStore};
use napi::bindgen_prelude::{AsyncTask, Buffer};
use napi::{Env, Task};
use napi_derive::napi;
use std::sync::Arc;

/// A panic inside the engine becomes an error answer instead of unwinding into Node (which would abort the process).
fn guarded(work: impl FnOnce() -> String) -> String {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(answer) => answer,
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown".into());
            serde_json::json!({"ok": false, "error": format!("Engine panic: {message}")})
                .to_string()
        }
    }
}

/// The clock for phase timings (`timings: true` on a request), as the native binaries install it; the layout never reads
/// it. Phase timings are collected per process, so with several calls running at once they add up across them.
fn install_clock() {
    static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    ORIGIN.get_or_init(std::time::Instant::now);
    arqlet_model::clock::install(|| {
        ORIGIN
            .get()
            .map_or(0., |o| o.elapsed().as_secs_f64() * 1000.)
    });
}

/// The engine: `dispatch` and a store of sessions. Provisional.
#[napi]
pub struct NativeEngine {
    store: Arc<SharedSessionStore>,
}

#[napi]
impl NativeEngine {
    /// A new engine with its own sessions. `sessionBytes` limits the sessions' estimated weight together (the least
    /// recently used idle session is dropped beyond it); without it there is no limit.
    #[napi(constructor)]
    pub fn new(session_bytes: Option<f64>) -> Self {
        install_clock();
        let limit = session_bytes
            .filter(|b| b.is_finite() && *b >= 0.)
            .map(|b| b as usize);
        NativeEngine {
            store: Arc::new(SharedSessionStore::with_limit(limit)),
        }
    }

    /// Runs one request (JSON text) through `dispatch` on the thread pool; resolves with the answer (JSON text).
    #[napi(ts_return_type = "Promise<string>")]
    pub fn run(&self, request: String) -> AsyncTask<RunTask> {
        AsyncTask::new(RunTask {
            request,
            session: None,
        })
    }

    /// Opens an empty session and answers its handle (never 0; handles are not reused).
    #[napi]
    pub fn open_session(&self) -> u32 {
        // Handles count up from 1; more than 2^32 sessions in one engine would wrap, which a host does not reach.
        self.store.open() as u32
    }

    /// Closes a session and frees its snapshot; answers whether it was open. A request running on it still answers.
    #[napi]
    pub fn close_session(&self, id: u32) -> bool {
        self.store.close(u64::from(id))
    }

    /// Runs one request (JSON text, without `snapshot`) on session `id` on the thread pool; resolves with the answer.
    #[napi(ts_return_type = "Promise<string>")]
    pub fn session_run(&self, id: u32, request: String) -> AsyncTask<RunTask> {
        AsyncTask::new(RunTask {
            request,
            session: Some((Arc::clone(&self.store), u64::from(id))),
        })
    }

    /// Runs a `columns` request, or an `overview` request with `columns: true` (`overview`), on session `id` and copies
    /// the numbers out under the same hold of the session; resolves with the answer and one buffer per listed column, in
    /// the order of the answer's `columns` list. The buffers are copies: they stay valid after later requests.
    #[napi(ts_return_type = "Promise<ColumnsAnswer>")]
    pub fn session_columns(
        &self,
        id: u32,
        request: String,
        overview: bool,
    ) -> AsyncTask<ColumnsTask> {
        AsyncTask::new(ColumnsTask {
            store: Arc::clone(&self.store),
            id: u64::from(id),
            request,
            overview,
        })
    }

    /// The open sessions: `{"ok": true, "sessions": [ids], "bytes": n, "evicted": n}` (JSON text).
    #[napi]
    pub fn sessions(&self) -> String {
        self.store.list()
    }
}

/// A request through `dispatch` or a session.
pub struct RunTask {
    request: String,
    session: Option<(Arc<SharedSessionStore>, u64)>,
}

impl Task for RunTask {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> napi::Result<String> {
        let request = std::mem::take(&mut self.request);
        Ok(guarded(|| match &self.session {
            // As the WASM engine without a publication callback: a progressive refinement's intermediate results are
            // dropped, its final answer is the same.
            None => arqlet_runtime::dispatch_streaming(&request, &mut |_| {}),
            Some((store, id)) => {
                let answer = store.run(*id, &request);
                with_heap(&request, answer)
            }
        }))
    }

    fn resolve(&mut self, _env: Env, output: String) -> napi::Result<String> {
        Ok(output)
    }
}

/// The answer to a columnar request: the JSON text and the columns' bytes, in the order the answer lists them.
#[napi(object)]
pub struct ColumnsAnswer {
    pub answer: String,
    pub columns: Vec<Buffer>,
}

pub struct ColumnsTask {
    store: Arc<SharedSessionStore>,
    id: u64,
    request: String,
    overview: bool,
}

impl Task for ColumnsTask {
    type Output = (String, Vec<Vec<u8>>);
    type JsValue = ColumnsAnswer;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let request = std::mem::take(&mut self.request);
        let overview = self.overview;
        let mut copied = Vec::new();
        let answer = guarded(|| {
            let outcome = self.store.with_session(self.id, |session| {
                let answer = session.run(&request);
                // The numbers as the WASM adapter exposes them (`lw_session_column`, `lw_session_overview_column`), copied
                // while the session is still held, so no later request can change them in between.
                copied = if overview {
                    session
                        .overview_columns()
                        .map(|c| c.columns().iter().map(|(_, col)| bytes_of(col)).collect())
                        .unwrap_or_default()
                } else {
                    session
                        .columns()
                        .map(|c| c.columns().iter().map(|(_, col)| bytes_of(col)).collect())
                        .unwrap_or_default()
                };
                answer
            });
            match outcome {
                Ok((answer, _)) => answer,
                Err(missing) => Missing::answer(missing),
            }
        });
        Ok((answer, copied))
    }

    fn resolve(
        &mut self,
        _env: Env,
        (answer, columns): Self::Output,
    ) -> napi::Result<ColumnsAnswer> {
        Ok(ColumnsAnswer {
            answer,
            columns: columns.into_iter().map(Buffer::from).collect(),
        })
    }
}

/// A column's numbers as bytes, in the platform's byte order (little-endian on every supported platform, as in WASM).
fn bytes_of(column: &arqlet_runtime::columns::Column<'_>) -> Vec<u8> {
    let (ptr, _) = column.raw();
    let len = column.bytes();
    if len == 0 {
        return Vec::new();
    }
    // SAFETY: `raw` points at the column's `bytes()` initialized bytes, owned by the session, which the caller holds
    // locked for the duration of this copy.
    unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
}

/// Provisional (ADR 0023, R7): the heap bytes this module has allocated and not freed, counted by a wrapper around the
/// system allocator (one atomic addition per allocation; it changes no result). Answered as `heapBytes` with `memory` and
/// `release`, as the WASM adapter does; there is no linear memory natively, so `linearMemoryBytes` is absent.
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

fn with_heap(request: &str, answer: String) -> String {
    let live = LIVE.load(std::sync::atomic::Ordering::Relaxed);
    arqlet_runtime::memory::with_adapter_heap(request, answer, live, None)
}
