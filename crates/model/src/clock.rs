//! Provisional (phase timings): a clock the host may install, and the time the layout spends in its phases.
//!
//! The core reads no clock of its own: a host that wants timings installs one (`install`: a native host from
//! `std::time::Instant`, the WASM module from the host's `performance.now`). The times are only reported, never used by the
//! layout, so the output stays the same with and without a clock. Without one every time is 0.

use std::sync::{Mutex, OnceLock};

static NOW: OnceLock<fn() -> f64> = OnceLock::new();

/// The phases the layout reports, in the order of `Phases`' fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Phases {
    /// Laying out the view, in ms (every phase below is part of it).
    pub layout: f64,
    pub placement: f64,
    pub organic_stress: f64,
    pub separation: f64,
    pub repair: f64,
    pub port_resolution: f64,
    pub routing: f64,
}

static SPENT: Mutex<Phases> = Mutex::new(Phases {
    layout: 0.,
    placement: 0.,
    organic_stress: 0.,
    separation: 0.,
    repair: 0.,
    port_resolution: 0.,
    routing: 0.,
});

/// Installs the host's clock (milliseconds from any fixed origin). The first one installed stays.
pub fn install(now: fn() -> f64) {
    let _ = NOW.set(now);
}

/// The host clock's time in ms, or `None` without a clock.
pub fn now() -> Option<f64> {
    NOW.get().map(|f| f())
}

/// Adds the times of one layout.
pub fn add(phases: Phases) {
    if let Ok(mut spent) = SPENT.lock() {
        spent.layout += phases.layout;
        spent.placement += phases.placement;
        spent.organic_stress += phases.organic_stress;
        spent.separation += phases.separation;
        spent.repair += phases.repair;
        spent.port_resolution += phases.port_resolution;
        spent.routing += phases.routing;
    }
}

/// The times added since the last `take`, and starts again from 0.
pub fn take() -> Phases {
    SPENT
        .lock()
        .map(|mut spent| std::mem::take(&mut *spent))
        .unwrap_or_default()
}
