//! Portable math: the transcendental functions the engine uses, computed the same way on every platform.
//!
//! `f64::sin`, `cos`, `atan2`, `powf` and `hypot` call the platform's math library natively (macOS, glibc, musl and
//! Windows differ in the last bits for some inputs) and Rust's own implementation in WASM. A layout that uses them is then
//! not the same on a Linux backend, a Mac and in the browser, which the split deployment mode and the determinism of the
//! engine require. These methods call the `libm` crate (a Rust port of musl's libm, MIT/Apache-2.0, no dependencies)
//! everywhere instead. `sqrt` and `powi` are exact or computed the same way on every platform and are not wrapped.
//!
//! Use the `p_` methods in the core crates instead of the `f64` ones.

/// The transcendental functions of `f64`, the same on every platform.
pub trait PortableMath {
    fn p_sin(self) -> f64;
    fn p_cos(self) -> f64;
    fn p_atan2(self, other: f64) -> f64;
    fn p_powf(self, n: f64) -> f64;
    fn p_hypot(self, other: f64) -> f64;
}

impl PortableMath for f64 {
    #[inline]
    fn p_sin(self) -> f64 {
        libm::sin(self)
    }
    #[inline]
    fn p_cos(self) -> f64 {
        libm::cos(self)
    }
    #[inline]
    fn p_atan2(self, other: f64) -> f64 {
        libm::atan2(self, other)
    }
    #[inline]
    fn p_powf(self, n: f64) -> f64 {
        libm::pow(self, n)
    }
    #[inline]
    fn p_hypot(self, other: f64) -> f64 {
        libm::hypot(self, other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portable_functions_compute_what_their_names_say() {
        assert!((1f64.p_hypot(1.) - 2f64.sqrt()).abs() < 1e-15);
        assert_eq!(3f64.p_hypot(4.), 5.);
        assert!((std::f64::consts::FRAC_PI_2.p_sin() - 1.).abs() < 1e-15);
        assert!((0f64.p_cos() - 1.).abs() < 1e-15);
        assert!((1f64.p_atan2(1.) - std::f64::consts::FRAC_PI_4).abs() < 1e-15);
        assert_eq!(2f64.p_powf(10.), 1024.);
    }
}
