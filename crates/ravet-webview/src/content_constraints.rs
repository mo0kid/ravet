//! Validated explicit content constraints, independent of a native window.
/// Finite logical-point bounds for an opt-in standalone content-size policy.
/// Values are copied, with no allocation or native access. This value and its
/// resolver are portable; native standalone window installation is macOS only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentConstraints {
    minimum_width: f64,
    minimum_height: f64,
    maximum_width: f64,
    maximum_height: f64,
}
impl ContentConstraints {
    /// Width bounds are 320..=4096; height bounds are 240..=4096.
    /// All values must be finite and each minimum must not exceed its maximum.
    pub fn new(
        minimum_width: f64,
        minimum_height: f64,
        maximum_width: f64,
        maximum_height: f64,
    ) -> Option<Self> {
        if !(320. ..=4096.).contains(&minimum_width)
            || !(320. ..=4096.).contains(&maximum_width)
            || !(240. ..=4096.).contains(&minimum_height)
            || !(240. ..=4096.).contains(&maximum_height)
            || minimum_width > maximum_width
            || minimum_height > maximum_height
        {
            return None;
        }
        Some(Self {
            minimum_width,
            minimum_height,
            maximum_width,
            maximum_height,
        })
    }
    /// Return the admitted minimum width and height in logical points.
    pub fn minimum_size(self) -> (f64, f64) {
        (self.minimum_width, self.minimum_height)
    }
    /// Return the admitted maximum width and height in logical points.
    pub fn maximum_size(self) -> (f64, f64) {
        (self.maximum_width, self.maximum_height)
    }
    /// Fit any finite positive requested extent inside this policy.
    /// Without aspect locking, dimensions clamp independently. An explicit finite
    /// positive `(width,height)` aspect fits the largest matching rectangle inside
    /// the request before clamping its scale to the compatible policy interval.
    /// Thus a too-small request can grow to the policy's minimum.
    ///
    /// Returns `None` for an invalid request/aspect, an empty compatible interval
    /// or unrepresentable arithmetic. Final scalar clamping contains floating-point
    /// edge rounding; it may perturb the aspect at machine precision. This pure
    /// result does not promise exact Cocoa geometry: native fitting rounds at
    /// logical-point precision, and callers measure actual content after pumping.
    pub fn resolve(
        self,
        width: f64,
        height: f64,
        aspect: Option<(f64, f64)>,
    ) -> Option<(f64, f64)> {
        if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
            return None;
        }
        let (width, height) = if let Some((aw, ah)) = aspect {
            if !aw.is_finite() || !ah.is_finite() || aw <= 0. || ah <= 0. {
                return None;
            }
            let lower = (self.minimum_width / aw).max(self.minimum_height / ah);
            let upper = (self.maximum_width / aw).min(self.maximum_height / ah);
            if !lower.is_finite() || !upper.is_finite() || lower <= 0. || lower > upper {
                return None;
            }
            let scale = (width / aw).min(height / ah).clamp(lower, upper);
            (aw * scale, ah * scale)
        } else {
            (width, height)
        };
        if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
            return None;
        }
        Some((
            width.clamp(self.minimum_width, self.maximum_width),
            height.clamp(self.minimum_height, self.maximum_height),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::ContentConstraints;
    #[test]
    fn exact_bounds_fractional_bounds_and_rejected_scalars() {
        let c = ContentConstraints::new(320., 240., 4096., 4096.).unwrap();
        assert_eq!(c.minimum_size(), (320., 240.));
        assert_eq!(c.maximum_size(), (4096., 4096.));
        assert!(ContentConstraints::new(400.25, 300.5, 1200.75, 900.125).is_some());
        assert!(ContentConstraints::new(640., 480., 640., 480.).is_some());
        for bad in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1.,
            0.,
            319.999,
            4096.001,
        ] {
            assert!(ContentConstraints::new(bad, 240., 4096., 4096.).is_none());
            assert!(ContentConstraints::new(320., 240., bad, 4096.).is_none());
        }
        for bad in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1.,
            0.,
            239.999,
            4096.001,
        ] {
            assert!(ContentConstraints::new(320., bad, 4096., 4096.).is_none());
            assert!(ContentConstraints::new(320., 240., 4096., bad).is_none());
        }
        assert!(ContentConstraints::new(800., 600., 799., 900.).is_none());
        assert!(ContentConstraints::new(800., 600., 1000., 599.).is_none());
    }
    #[test]
    fn independent_requests_clamp_without_allocation_or_native_policy() {
        let c = ContentConstraints::new(640.25, 480.5, 1400.75, 1000.125).unwrap();
        assert_eq!(c.resolve(320., 240., None), Some(c.minimum_size()));
        assert_eq!(c.resolve(4096., 4096., None), Some(c.maximum_size()));
        assert_eq!(c.resolve(1200., 700., None), Some((1200., 700.)));
        assert_eq!(c.resolve(1500., 200., None), Some((1400.75, 480.5)));
        assert_eq!(c.resolve(f64::MAX, f64::MAX, None), Some(c.maximum_size()));
        {
            for n in 1..10000 {
                let pair = c.resolve(f64::from(n), f64::from(n) * 0.7, None).unwrap();
                assert!((640.25..=1400.75).contains(&pair.0));
                assert!((480.5..=1000.125).contains(&pair.1));
            }
        }
    }
    #[test]
    fn aspect_interval_fits_then_clamps_and_accepts_exact_fixed_policy() {
        let c = ContentConstraints::new(640., 480., 1600., 1200.).unwrap();
        for (request, expected) in [
            ((1000., 1000.), (1000., 750.)),
            ((320., 240.), (640., 480.)),
            ((4096., 4096.), (1600., 1200.)),
            ((1400., 600.), (800., 600.)),
        ] {
            assert_eq!(
                c.resolve(request.0, request.1, Some((4., 3.))),
                Some(expected)
            );
        }
        let fixed = ContentConstraints::new(640., 480., 640., 480.).unwrap();
        assert_eq!(
            fixed.resolve(1000., 1000., Some((4., 3.))),
            Some((640., 480.))
        );
        assert!(fixed.resolve(1000., 1000., Some((16., 9.))).is_none());
        let extreme = ContentConstraints::new(320., 240., 4096., 4096.).unwrap();
        let pair = extreme
            .resolve(1000., 1200., Some((f64::MAX, f64::MAX)))
            .unwrap();
        assert!((pair.0 - 1000.).abs() < 1e-10);
        assert_eq!(pair.0, pair.1);
    }
    #[test]
    fn invalid_requests_aspects_and_unrepresentable_intervals_reject() {
        let c = ContentConstraints::new(640., 480., 1200., 900.).unwrap();
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1., 0.] {
            assert!(c.resolve(bad, 600., None).is_none());
            assert!(c.resolve(800., bad, None).is_none());
            assert!(c.resolve(800., 600., Some((bad, 3.))).is_none());
            assert!(c.resolve(800., 600., Some((4., bad))).is_none());
        }
        assert!(c.resolve(800., 600., Some((1., 100.))).is_none());
        assert!(c.resolve(800., 600., Some((100., 1.))).is_none());
        assert!(
            c.resolve(800., 600., Some((f64::from_bits(1), 1.)))
                .is_none()
        );
        assert_eq!(c.minimum_size(), (640., 480.));
        assert_eq!(c.maximum_size(), (1200., 900.));
    }
    #[test]
    fn two_dimensional_resize_sweep_preserves_caps_and_aspect_interval() {
        let c = ContentConstraints::new(640., 480., 1600., 1200.).unwrap();
        {
            for n in 1..10000 {
                let width = f64::from((n * 97) % 5000 + 1);
                let height = f64::from((n * 71) % 5000 + 1);
                let p = c.resolve(width, height, Some((4., 3.))).unwrap();
                assert!((640. - 1e-10..=1600. + 1e-10).contains(&p.0));
                assert!((480. - 1e-10..=1200. + 1e-10).contains(&p.1));
                assert!((p.0 - p.1 * 4. / 3.).abs() < 1e-10);
            }
        }
    }
}
