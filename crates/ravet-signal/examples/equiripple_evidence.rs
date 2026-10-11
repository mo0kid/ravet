//! Headless generated coefficient/support evidence for the independent Decimal oracle.
use ravet_signal::{EquirippleFirDesign as D, EquirippleFirResponse as R, EquirippleOptions as O};
fn number(x: f64) -> String {
    format!("{x:.17e}")
}
fn main() {
    let cases = [
        (
            R::LowPass {
                pass_hz: 5760.,
                stop_hz: 7200.,
            },
            vec![[0., 0.12, 1., 1., 0.5], [0.15, 0.5, 0., 0., 1.]],
        ),
        (
            R::HighPass {
                stop_hz: 4320.,
                pass_hz: 6240.,
            },
            vec![[0., 0.09, 0., 0., 1.], [0.13, 0.5, 1., 1., 0.5]],
        ),
        (
            R::BandPass {
                low_stop_hz: 2880.,
                low_pass_hz: 4800.,
                high_pass_hz: 12480.,
                high_stop_hz: 14400.,
            },
            vec![
                [0., 0.06, 0., 0., 1.],
                [0.10, 0.26, 1., 1., 0.5],
                [0.30, 0.5, 0., 0., 1.],
            ],
        ),
        (
            R::BandStop {
                low_pass_hz: 2880.,
                low_stop_hz: 4800.,
                high_stop_hz: 12480.,
                high_pass_hz: 14400.,
            },
            vec![
                [0., 0.06, 1., 1., 0.5],
                [0.10, 0.26, 0., 0., 1.],
                [0.30, 0.5, 1., 1., 0.5],
            ],
        ),
    ];
    let mut records = Vec::new();
    for (taps, model) in [(63, "odd"), (64, "even")] {
        for (r, bands) in &cases {
            if taps == 64 && matches!(r, R::HighPass { .. } | R::BandStop { .. }) {
                continue;
            }
            let d = D::new(*r, 48000., taps, 1., 2., O::default()).unwrap();
            records.push(record(&d, taps, model, 48000., bands));
        }
    }
    let d = D::halfband(63, 0.08, O::default()).unwrap();
    records.push(record(&d, 63, "half", 1., &[[0., 0.21, 1., 1., 1.]]));
    for (taps, model) in [(511, "odd"), (512, "even")] {
        let d = D::new(
            R::LowPass {
                pass_hz: 7000.,
                stop_hz: 7240.,
            },
            48000.,
            taps,
            1.,
            2.,
            O::default(),
        )
        .unwrap();
        records.push(record(
            &d,
            taps,
            model,
            48000.,
            &[
                [0., 7000. / 48000., 1., 1., 0.5],
                [7240. / 48000., 0.5, 0., 0., 1.],
            ],
        ));
    }
    println!("[{}]", records.join(",\n"));
}
fn record(d: &D, taps: usize, model: &str, rate: f64, bands: &[[f64; 5]]) -> String {
    let h = d
        .design()
        .coefficients()
        .iter()
        .map(|&v| number(v))
        .collect::<Vec<_>>()
        .join(",");
    let bands = bands
        .iter()
        .map(|b| {
            format!(
                "[{}]",
                b.iter().map(|&v| number(v)).collect::<Vec<_>>().join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let extrema = d
        .extrema()
        .iter()
        .map(|e| {
            format!(
                "[{},{}]",
                number(e.frequency_hz / rate),
                number(e.normalized_error)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let reflected = d
        .reflected_halfband_extrema()
        .map(|e| {
            format!(
                "[{},{}]",
                number(e.frequency_hz / rate),
                number(e.normalized_error)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"taps\":{taps},\"model\":\"{model}\",\"density\":{},\"iterations\":{},\"grid_points\":{},\"normalized_error\":{},\"ripple\":{},\"allowance\":{},\"coefficients\":[{h}],\"bands\":[{bands}],\"extrema\":[{extrema}],\"reflected_extrema\":[{reflected}]}}",
        d.grid_density(),
        d.iterations(),
        d.grid_points(),
        number(d.normalized_grid_error()),
        number(d.normalized_ripple()),
        number(d.normalized_rounding_allowance())
    )
}
