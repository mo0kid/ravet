use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, EventError, ParameterId, PrepareError,
    ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor, Tail, Transport,
};
use ravet_graph::{MonoDuplicator, MonoFactoryError, SharedMonoKernel};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
struct Controls {
    gain: f64,
    fail: Option<usize>,
    panic_prepare: Option<usize>,
    trace: Arc<Mutex<Vec<(usize, &'static str)>>>,
    drops: Arc<AtomicUsize>,
}
struct Kernel {
    id: usize,
    state: f64,
    calls: usize,
    address: usize,
    active: bool,
    trace: Arc<Mutex<Vec<(usize, &'static str)>>>,
    drops: Arc<AtomicUsize>,
}
macro_rules! kernel {
    ($s:ty) => {
        impl SharedMonoKernel<$s, Controls> for Kernel {
            fn prepare(&mut self, _: f64, _: usize, c: &Controls) -> Result<(), PrepareError> {
                self.trace.lock().unwrap().push((self.id, "prepare"));
                if c.panic_prepare == Some(self.id) {
                    panic!("kernel prepare");
                }
                if c.fail == Some(self.id) {
                    return Err(PrepareError::InvalidSmoothingTime);
                }
                self.active = true;
                self.address = c as *const _ as usize;
                Ok(())
            }
            fn process_sample(&mut self, x: $s, c: &Controls) -> $s {
                assert!(self.active);
                assert_eq!(self.address, c as *const _ as usize);
                self.calls += 1;
                let y = x * c.gain as $s + self.state as $s;
                self.state = x as f64;
                y
            }
            fn reset(&mut self, _: &Controls) {
                self.state = 0.;
            }
            fn deactivate(&mut self) {
                assert!(self.active);
                self.trace.lock().unwrap().push((self.id, "deactivate"));
                self.active = false;
            }
        }
    };
}
kernel!(f32);
kernel!(f64);
impl Drop for Controls {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
impl Drop for Kernel {
    fn drop(&mut self) {
        assert!(!self.active);
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn controls(fail: Option<usize>) -> Controls {
    Controls {
        gain: 2.,
        fail,
        panic_prepare: None,
        trace: Arc::new(Mutex::new(Vec::new())),
        drops: Arc::new(AtomicUsize::new(0)),
    }
}
fn factory(i: usize, c: &Controls) -> Result<Kernel, &'static str> {
    Ok(Kernel {
        id: i,
        state: 0.,
        calls: 0,
        address: 0,
        active: false,
        trace: c.trace.clone(),
        drops: c.drops.clone(),
    })
}
fn config(ch: usize) -> ProcessConfig {
    ProcessConfig::new(48000., 8, 0, AudioLayout::new(ch, ch).unwrap()).unwrap()
}
fn ids(log: &Mutex<Vec<(usize, &'static str)>>, what: &str) -> Vec<usize> {
    log.lock()
        .unwrap()
        .iter()
        .filter(|x| x.1 == what)
        .map(|x| x.0)
        .collect()
}
fn views<S>(rows: &mut [Vec<S>]) -> Vec<&mut [S]> {
    rows.iter_mut().map(Vec::as_mut_slice).collect()
}
macro_rules! family {
    ($name:ident,$s:ty) => {
        #[test]
        fn $name() {
            for channels in 1..=64 {
                let mut d = MonoDuplicator::<$s, _, _>::new(controls(None), channels, 8, factory)
                    .ok()
                    .unwrap();
                d.prepare(config(channels)).unwrap();
                let input: Vec<$s> = (0..channels).map(|x| x as $s + 1.).collect();
                let mut out = vec![99.; channels];
                d.process_frame(&input, &mut out).unwrap();
                for (i, y) in out.iter().enumerate() {
                    assert_eq!(*y, input[i] * 2.);
                    assert_eq!(d.kernel(i).unwrap().calls, 1);
                    assert_eq!(
                        d.kernel(i).unwrap().address,
                        d.shared_controls() as *const _ as usize
                    );
                }
                let mut block: Vec<Vec<$s>> = (0..channels)
                    .map(|i| vec![i as $s + 1., i as $s + 2., 99.])
                    .collect();
                let sources = block.clone();
                let mut views: Vec<&mut [$s]> = block.iter_mut().map(Vec::as_mut_slice).collect();
                d.process_in_place(&mut views, 2).unwrap();
                for i in 0..channels {
                    assert_eq!(block[i][0], sources[i][0] * 2. + input[i]);
                    assert_eq!(block[i][1], sources[i][1] * 2. + sources[i][0]);
                    assert_eq!(block[i][2], 99.);
                }
                let before: Vec<_> = (0..channels).map(|i| d.kernel(i).unwrap().calls).collect();
                d.set_bypassed(true);
                d.process_frame(&input, &mut out).unwrap();
                assert_eq!(input, out);
                for i in 0..channels {
                    assert_eq!(d.kernel(i).unwrap().calls, before[i]);
                }
                d.reset();
                for i in 0..channels {
                    assert_eq!(d.kernel(i).unwrap().state, 0.);
                }
                d.set_bypassed(false);
                d.shared_controls_mut_stopped().gain = 3.;
                assert_eq!(
                    d.process_frame(&input, &mut out),
                    Err(ProcessError::NotPrepared)
                );
                d.prepare(config(channels)).unwrap();
                d.process_frame(&input, &mut out).unwrap();
                for i in 0..channels {
                    assert_eq!(out[i], input[i] * 3.);
                }
                let drops = d.shared_controls().drops.clone();
                let (c, ks) = d.into_parts();
                assert_eq!(ks.len(), channels);
                assert_eq!(drops.load(Ordering::SeqCst), 0);
                assert_eq!(c.gain, 3.);
                drop((c, ks));
                assert_eq!(drops.load(Ordering::SeqCst), channels + 1);
            }
        }
    };
}
family!(
    all64_native32_channels_shared_controls_independent_histories,
    f32
);
family!(
    all64_native64_channels_shared_controls_independent_histories,
    f64
);
#[test]
fn prepare_each_failure_reverse_deactivation_and_exact_control_owner() {
    for channels in [1, 2, 7, 64] {
        for failed in 0..channels {
            let c = controls(Some(failed));
            let drops = c.drops.clone();
            let mut d = MonoDuplicator::<f32, _, _>::new(c, channels, 8, factory)
                .ok()
                .unwrap();
            assert_eq!(
                d.prepare(config(channels)),
                Err(PrepareError::InvalidSmoothingTime)
            );
            assert_eq!(
                d.prepare_failure(),
                Some((failed, PrepareError::InvalidSmoothingTime))
            );
            let log = d.shared_controls().trace.lock().unwrap();
            assert_eq!(
                log.iter()
                    .filter(|x| x.1 == "prepare")
                    .map(|x| x.0)
                    .collect::<Vec<_>>(),
                (0..=failed).collect::<Vec<_>>()
            );
            assert_eq!(
                log.iter()
                    .filter(|x| x.1 == "deactivate")
                    .map(|x| x.0)
                    .collect::<Vec<_>>(),
                (0..failed).rev().collect::<Vec<_>>()
            );
            drop(log);
            d.deactivate();
            drop(d);
            assert_eq!(drops.load(Ordering::SeqCst), channels + 1);
        }
    }
}
#[test]
fn factory_prefix_original_error_controls_and_callable_factory_recovered() {
    for failed in 0..64 {
        let c = controls(None);
        let drops = c.drops.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let mut failure = MonoDuplicator::<f64, _, Kernel>::new(c, 64, 8, move |i, c| {
            observed.fetch_add(1, Ordering::SeqCst);
            if i == failed {
                Err("original")
            } else {
                factory(i, c)
            }
        })
        .err()
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), failed + 1);
        assert_eq!(failure.kernels.len(), failed);
        assert_eq!(
            failure.error,
            MonoFactoryError::Factory {
                index: failed,
                error: "original"
            }
        );
        assert_eq!(
            (failure.factory)(failed, &failure.controls).err(),
            Some("original")
        );
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        drop(failure);
        assert_eq!(drops.load(Ordering::SeqCst), failed + 1);
    }
}
#[test]
fn whole_planar_admission_rejects_last_channel_before_any_history() {
    let mut d = MonoDuplicator::<f64, _, _>::new(controls(None), 3, 8, factory)
        .ok()
        .unwrap();
    d.prepare(config(3)).unwrap();
    let a = [1., 2., 3.];
    let mut x = [9.; 3];
    let mut y = [9.; 3];
    let mut z = [9.; 2];
    let input = [&a[..], &a[..], &a[..]];
    let mut output = [&mut x[..], &mut y[..], &mut z[..]];
    assert_eq!(
        d.process_planar(&input, &mut output, 3),
        Err(ProcessError::LayoutMismatch)
    );
    for i in 0..3 {
        assert_eq!(d.kernel(i).unwrap().calls, 0);
    }
    assert_eq!(x, [9.; 3]);
    assert_eq!(y, [9.; 3]);
    assert_eq!(z, [9.; 2]);
    let mut z = [9.; 3];
    let mut output = [&mut x[..], &mut y[..], &mut z[..]];
    d.process_planar(&input, &mut output, 2).unwrap();
    assert_eq!(output[0], [2., 5., 9.]);
    assert_eq!(output[1], output[0]);
    assert_eq!(output[2], output[0]);
    let before = d.kernel(0).unwrap().calls;
    d.process_planar(&input, &mut output, 0).unwrap();
    assert_eq!(d.kernel(0).unwrap().calls, before);
    let mut empty = [];
    assert_eq!(
        d.process_frame(&a, &mut empty),
        Err(ProcessError::LayoutMismatch)
    );
    assert_eq!(d.process_sample(3, 2.), Err(ProcessError::LayoutMismatch));
}
#[test]
fn constructor_and_config_reject_before_factory_or_kernel_prepare() {
    for (ch, max, error) in [
        (0, 8, MonoFactoryError::<&str>::Channels),
        (65, 8, MonoFactoryError::Channels),
        (1, 0, MonoFactoryError::Frames),
        (1, 65537, MonoFactoryError::Frames),
    ] {
        let c = controls(None);
        let calls = AtomicUsize::new(0);
        let failure = MonoDuplicator::<f32, _, Kernel>::new(c, ch, max, |i, c| {
            calls.fetch_add(1, Ordering::SeqCst);
            factory(i, c)
        })
        .err()
        .unwrap();
        assert_eq!(failure.error, error);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(failure.kernels.is_empty());
    }
    let mut d = MonoDuplicator::<f64, _, _>::new(controls(None), 2, 8, factory)
        .ok()
        .unwrap();
    for c in [
        config(1),
        config(2).with_output_events(1),
        ProcessConfig::new(48000., 8, 1, AudioLayout::STEREO).unwrap(),
        config(2)
            .with_auxiliary_buses(&[AudioLayout::MONO])
            .unwrap(),
    ] {
        assert_eq!(d.prepare(c), Err(PrepareError::UnsupportedLayout));
    }
    assert!(d.shared_controls().trace.lock().unwrap().is_empty());
}
#[test]
fn declared_storage_budget_refuses_before_factory_or_reservation() {
    #[repr(align(8388608))]
    struct Huge {
        _value: u8,
    }
    impl SharedMonoKernel<f64, ()> for Huge {
        fn prepare(&mut self, _: f64, _: usize, _: &()) -> Result<(), PrepareError> {
            Ok(())
        }
        fn process_sample(&mut self, x: f64, _: &()) -> f64 {
            x
        }
        fn reset(&mut self, _: &()) {}
        fn deactivate(&mut self) {}
    }
    std::thread::Builder::new()
        .stack_size(128 * 1024 * 1024)
        .spawn(|| {
            let calls = AtomicUsize::new(0);
            let result = MonoDuplicator::<f64, (), Huge>::new((), 1, 8, |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<Huge, _>("never")
            })
            .err()
            .unwrap();
            assert_eq!(result.error, MonoFactoryError::Capacity);
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            assert_eq!(result.kernels.capacity(), 0);
        })
        .unwrap()
        .join()
        .unwrap();
}
#[test]
fn foreign_factory_unwind_drops_only_constructed_prefix_and_original_controls() {
    for failed in [0, 1, 7, 63] {
        let c = controls(None);
        let drops = c.drops.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = MonoDuplicator::<f64, _, Kernel>::new(c, 64, 8, |i, c| {
                if i == failed {
                    panic!("factory");
                }
                factory(i, c)
            });
        }));
        assert!(result.is_err());
        assert_eq!(drops.load(Ordering::SeqCst), failed + 1);
    }
}
// Direct `Processor::process` coverage in both native precisions. The test
// kernel computes y = gain * x + previous x, so every output exposes the history
// that preceded it and any skipped or extra kernel call shows up in later blocks.
macro_rules! direct {
    ($m:ident, $s:ty) => {
        mod $m {
            use super::*;
            type Dup = MonoDuplicator<$s, Controls, Kernel>;
            fn rows(v: &[&[$s]]) -> Vec<Vec<$s>> {
                v.iter().map(|r| r.to_vec()).collect()
            }
            fn prepared(channels: usize) -> Box<Dup> {
                // The kernel records the inline controls' address at prepare.
                // Put the fixture in its final location before preparing it;
                // returning the Box moves ownership without moving those controls.
                let mut d = Box::new(Dup::new(controls(None), channels, 8, factory).ok().unwrap());
                d.prepare(config(channels)).unwrap();
                d
            }
            fn in_place(d: &mut Dup, data: &mut [Vec<$s>], frames: usize) -> ProcessStatus {
                let mut v = views(data);
                let audio = AudioBuffer::in_place(&mut v, frames).unwrap();
                d.process(&mut ProcessBlock::new(audio, &[], Transport::default()))
            }
            fn separate(
                d: &mut Dup,
                input: &[Vec<$s>],
                output: &mut [Vec<$s>],
                frames: usize,
            ) -> ProcessStatus {
                let inputs: Vec<&[$s]> = input.iter().map(Vec::as_slice).collect();
                let mut outputs = views(output);
                let audio = AudioBuffer::separate(&inputs, &mut outputs, frames).unwrap();
                d.process(&mut ProcessBlock::new(audio, &[], Transport::default()))
            }
            fn with_auxiliary(d: &mut Dup, data: &mut [Vec<$s>], frames: usize) -> ProcessStatus {
                let mut v = views(data);
                let audio = AudioBuffer::in_place(&mut v, frames).unwrap();
                let mut aux_data: Vec<$s> = vec![5.; frames];
                let mut aux_views = [&mut aux_data[..]];
                let mut aux = [AudioBuffer::in_place(&mut aux_views, frames).unwrap()];
                let mut block = ProcessBlock::new(audio, &[], Transport::default());
                block.auxiliary = Some(&mut aux);
                d.process(&mut block)
            }
            fn calls(d: &Dup) -> Vec<usize> {
                (0..d.channels())
                    .map(|i| d.kernel(i).unwrap().calls)
                    .collect()
            }
            #[test]
            fn zero_and_variable_frames_carry_independent_history_and_preserve_suffix() {
                let mut d = Dup::new(controls(None), 2, 8, factory).ok().unwrap();
                // Unprepared: active frames are silenced, the suffix survives, no kernel runs.
                let mut data = rows(&[&[1., 2., 3., 99.], &[4., 5., 6., 99.]]);
                assert_eq!(
                    in_place(&mut d, &mut data, 3),
                    ProcessStatus::Fault(ProcessError::NotPrepared)
                );
                assert_eq!(data, rows(&[&[0., 0., 0., 99.], &[0., 0., 0., 99.]]));
                assert_eq!(calls(&d), [0, 0]);
                d.prepare(config(2)).unwrap();
                // A zero-frame block reads and writes nothing.
                let mut data = rows(&[&[7., 8.], &[9., 10.]]);
                assert_eq!(in_place(&mut d, &mut data, 0), ProcessStatus::Continue);
                assert_eq!(data, rows(&[&[7., 8.], &[9., 10.]]));
                assert_eq!(calls(&d), [0, 0]);
                // Variable block sizes: y = 2x + previous x, per channel.
                let mut data = rows(&[&[1., 2., 3., 99.], &[10., 20., 30., 99.]]);
                assert_eq!(in_place(&mut d, &mut data, 3), ProcessStatus::Continue);
                assert_eq!(data, rows(&[&[2., 5., 8., 99.], &[20., 50., 80., 99.]]));
                let mut data = rows(&[&[4., 5., 99.], &[40., 50., 99.]]);
                assert_eq!(in_place(&mut d, &mut data, 2), ProcessStatus::Continue);
                assert_eq!(data, rows(&[&[11., 14., 99.], &[110., 140., 99.]]));
                let mut data = rows(&[&[0.], &[0.]]);
                assert_eq!(in_place(&mut d, &mut data, 0), ProcessStatus::Continue);
                assert_eq!(calls(&d), [5, 5]);
                // Previous inputs are now 5 and 50.
                let mut data = rows(&[&[6., 99.], &[60., 99.]]);
                assert_eq!(in_place(&mut d, &mut data, 1), ProcessStatus::Continue);
                assert_eq!(data, rows(&[&[17., 99.], &[170., 99.]]));
            }
            #[test]
            fn refused_blocks_silence_active_frames_and_leave_histories_and_counters() {
                let mut d = prepared(2);
                let mut warm = rows(&[&[4., 5.], &[40., 50.]]);
                assert_eq!(in_place(&mut d, &mut warm, 2), ProcessStatus::Continue);
                let counters = calls(&d);
                // Frames above the prepared maximum: silence the 9 requested frames only.
                let mut data: Vec<Vec<$s>> = (0..2)
                    .map(|_| {
                        let mut v = vec![5.; 9];
                        v.push(99.);
                        v
                    })
                    .collect();
                assert_eq!(
                    in_place(&mut d, &mut data, 9),
                    ProcessStatus::Fault(ProcessError::TooManyFrames {
                        frames: 9,
                        maximum: 8
                    })
                );
                for row in &data {
                    assert!(row[..9].iter().all(|x| *x == 0.));
                    assert_eq!(row[9], 99.);
                }
                // Wrong channel count.
                let mut data = rows(&[&[1., 2., 99.]]);
                assert_eq!(
                    in_place(&mut d, &mut data, 2),
                    ProcessStatus::Fault(ProcessError::LayoutMismatch)
                );
                assert_eq!(data, rows(&[&[0., 0., 99.]]));
                // Input events are rejected: this profile prepares with none.
                let event = [Event {
                    frame_offset: 0,
                    data: EventData::ParameterValue {
                        id: ParameterId(1),
                        value: 1.,
                    },
                }];
                let mut data = rows(&[&[1., 2., 99.], &[3., 4., 99.]]);
                let status = {
                    let mut v = views(&mut data);
                    let audio = AudioBuffer::in_place(&mut v, 2).unwrap();
                    d.process(&mut ProcessBlock::new(audio, &event, Transport::default()))
                };
                assert_eq!(
                    status,
                    ProcessStatus::Fault(ProcessError::Events(EventError::TooMany {
                        count: 1,
                        maximum: 0
                    }))
                );
                assert_eq!(data, rows(&[&[0., 0., 99.], &[0., 0., 99.]]));
                // An auxiliary bus is a layout mismatch against a prepare without any.
                let mut data = rows(&[&[1., 2., 99.], &[3., 4., 99.]]);
                assert_eq!(
                    with_auxiliary(&mut d, &mut data, 2),
                    ProcessStatus::Fault(ProcessError::LayoutMismatch)
                );
                assert_eq!(data, rows(&[&[0., 0., 99.], &[0., 0., 99.]]));
                // No refusal ran a kernel or touched a history.
                assert_eq!(calls(&d), counters);
                assert_eq!(d.kernel(0).unwrap().state, 5.);
                assert_eq!(d.kernel(1).unwrap().state, 50.);
                let mut data = rows(&[&[6., 99.], &[60., 99.]]);
                assert_eq!(in_place(&mut d, &mut data, 1), ProcessStatus::Continue);
                assert_eq!(data, rows(&[&[17., 99.], &[170., 99.]]));
            }
            #[test]
            fn separate_buffers_leave_inputs_alone_and_preserve_output_suffix() {
                let mut d = prepared(2);
                let input = rows(&[&[1., 2., 3., 77.], &[10., 20., 30., 77.]]);
                let mut output = rows(&[&[55.; 4], &[55.; 4]]);
                assert_eq!(
                    separate(&mut d, &input, &mut output, 3),
                    ProcessStatus::Continue
                );
                assert_eq!(output, rows(&[&[2., 5., 8., 55.], &[20., 50., 80., 55.]]));
                assert_eq!(input, rows(&[&[1., 2., 3., 77.], &[10., 20., 30., 77.]]));
            }
            #[test]
            fn bypass_is_exact_dry_and_freezes_kernel_state_until_resumed() {
                let mut d = prepared(2);
                let mut warm = rows(&[&[1., 2., 3.], &[10., 20., 30.]]);
                in_place(&mut d, &mut warm, 3);
                let counters = calls(&d);
                d.set_bypassed(true);
                let mut data = rows(&[&[5., 6., 7., 99.], &[8., 9., 10., 99.]]);
                assert_eq!(in_place(&mut d, &mut data, 3), ProcessStatus::Continue);
                assert_eq!(data, rows(&[&[5., 6., 7., 99.], &[8., 9., 10., 99.]]));
                let input = rows(&[&[5., 6., 7., 77.], &[8., 9., 10., 77.]]);
                let mut output = rows(&[&[55.; 4], &[55.; 4]]);
                assert_eq!(
                    separate(&mut d, &input, &mut output, 3),
                    ProcessStatus::Continue
                );
                assert_eq!(output, rows(&[&[5., 6., 7., 55.], &[8., 9., 10., 55.]]));
                assert_eq!(calls(&d), counters);
                assert_eq!(d.kernel(0).unwrap().state, 3.);
                assert_eq!(d.kernel(1).unwrap().state, 30.);
                // Bypass does not reset: resuming continues from the frozen history.
                d.set_bypassed(false);
                let mut data = rows(&[&[4.], &[40.]]);
                in_place(&mut d, &mut data, 1);
                assert_eq!(data, rows(&[&[11.], &[110.]]));
            }
            #[test]
            fn nonfinite_samples_are_the_kernels_policy_not_filtered_or_rejected() {
                let mut d = prepared(2);
                let mut data = rows(&[&[<$s>::NAN, 1., 2., 99.], &[1., 2., 3., 99.]]);
                assert_eq!(in_place(&mut d, &mut data, 3), ProcessStatus::Continue);
                // Channel 0's kernel leaks NaN into its own next sample via history only.
                assert!(data[0][0].is_nan());
                assert!(data[0][1].is_nan());
                assert_eq!(data[0][2], 5.);
                assert_eq!(data[0][3], 99.);
                assert_eq!(data[1], rows(&[&[2., 5., 8., 99.]])[0]);
                assert_eq!(calls(&d), [3, 3]);
                let mut data = rows(&[&[<$s>::INFINITY, 1., 2.], &[1., 2., 3.]]);
                assert_eq!(in_place(&mut d, &mut data, 3), ProcessStatus::Continue);
                assert_eq!(data[0][0], <$s>::INFINITY);
                assert_eq!(data[0][1], <$s>::INFINITY);
                assert_eq!(data[0][2], 5.);
                // Bypass forwards nonfinite input without running a kernel.
                d.set_bypassed(true);
                let mut data = rows(&[&[<$s>::NAN, <$s>::NEG_INFINITY], &[1., 2.]]);
                assert_eq!(in_place(&mut d, &mut data, 2), ProcessStatus::Continue);
                assert!(data[0][0].is_nan());
                assert_eq!(data[0][1], <$s>::NEG_INFINITY);
                assert_eq!(calls(&d), [6, 6]);
            }
            #[test]
            fn direct_render_and_bypass_paths_allocate_and_deallocate_nothing() {
                let mut d = prepared(2);
                let src: [[$s; 8]; 2] = [[1.; 8], [2.; 8]];
                let mut dst: [[$s; 8]; 2] = [[0.; 8]; 2];
                let mut inplace: [[$s; 8]; 2] = [[3.; 8]; 2];
                let mut direct: [[$s; 8]; 2] = [[4.; 8]; 2];
                let info = allocation_counter::measure(|| {
                    for (bypass, frames) in
                        [(false, 8), (false, 0), (false, 3), (true, 8), (true, 1)]
                    {
                        d.set_bypassed(bypass);
                        let inputs = [&src[0][..], &src[1][..]];
                        let [o0, o1] = &mut dst;
                        let mut outputs = [&mut o0[..], &mut o1[..]];
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, frames).unwrap();
                        assert_eq!(
                            d.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                            ProcessStatus::Continue
                        );
                        let [i0, i1] = &mut inplace;
                        let mut v = [&mut i0[..], &mut i1[..]];
                        let audio = AudioBuffer::in_place(&mut v, frames).unwrap();
                        assert_eq!(
                            d.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                            ProcessStatus::Continue
                        );
                        d.process_planar(&inputs, &mut outputs, frames).unwrap();
                        let [d0, d1] = &mut direct;
                        let mut dv = [&mut d0[..], &mut d1[..]];
                        d.process_in_place(&mut dv, frames).unwrap();
                        let mut out = [0.; 2];
                        d.process_frame(&[1., 2.], &mut out).unwrap();
                        d.process_sample(0, 1.).unwrap();
                    }
                });
                assert_eq!(info.count_total, 0);
                assert_eq!(info.count_current, 0);
            }
        }
    };
}
direct!(native32_direct, f32);
direct!(native64_direct, f64);
#[test]
fn planar_and_in_place_preflight_whole_call_with_variable_frames_and_bypass() {
    let mut d = MonoDuplicator::<f64, _, _>::new(controls(None), 3, 8, factory)
        .ok()
        .unwrap();
    let a = [1., 2., 3., 4.];
    let b = [10., 20., 30., 40.];
    let c = [100., 200., 300., 400.];
    let short = [1., 2.];
    let inputs = [&a[..], &b[..], &c[..]];
    let mut x = [9.; 5];
    let mut y = [9.; 5];
    let mut z = [9.; 5];
    {
        let mut outputs = [&mut x[..], &mut y[..], &mut z[..]];
        assert_eq!(
            d.process_planar(&inputs, &mut outputs, 2),
            Err(ProcessError::NotPrepared)
        );
    }
    d.prepare(config(3)).unwrap();
    {
        let mut outputs = [&mut x[..], &mut y[..], &mut z[..]];
        assert_eq!(
            d.process_planar(&inputs, &mut outputs, 9),
            Err(ProcessError::TooManyFrames {
                frames: 9,
                maximum: 8
            })
        );
        let first_short = [&short[..], &b[..], &c[..]];
        let last_short = [&a[..], &b[..], &short[..]];
        for bad in [first_short, last_short] {
            assert_eq!(
                d.process_planar(&bad, &mut outputs, 3),
                Err(ProcessError::LayoutMismatch)
            );
        }
        assert_eq!(
            d.process_planar(&inputs[..2], &mut outputs, 3),
            Err(ProcessError::LayoutMismatch)
        );
        assert_eq!(
            d.process_planar(&inputs, &mut outputs[..2], 3),
            Err(ProcessError::LayoutMismatch)
        );
    }
    // None of the refusals wrote an output or advanced a kernel.
    assert_eq!((x, y, z), ([9.; 5], [9.; 5], [9.; 5]));
    for i in 0..3 {
        assert_eq!(d.kernel(i).unwrap().calls, 0);
    }
    {
        let mut outputs = [&mut x[..], &mut y[..], &mut z[..]];
        d.process_planar(&inputs, &mut outputs, 3).unwrap();
        d.process_planar(&inputs, &mut outputs, 0).unwrap();
        d.process_planar(&inputs, &mut outputs, 1).unwrap();
    }
    // 3 frames (y = 2x + previous x), then none, then 1 frame after previous input 3.
    assert_eq!(x, [5., 5., 8., 9., 9.]);
    assert_eq!(y, [50., 50., 80., 9., 9.]);
    assert_eq!(z, [500., 500., 800., 9., 9.]);
    assert_eq!(d.kernel(0).unwrap().calls, 4);
    d.set_bypassed(true);
    {
        let mut outputs = [&mut x[..], &mut y[..], &mut z[..]];
        d.process_planar(&inputs, &mut outputs, 4).unwrap();
    }
    assert_eq!(x, [1., 2., 3., 4., 9.]);
    assert_eq!(y, [10., 20., 30., 40., 9.]);
    assert_eq!(z, [100., 200., 300., 400., 9.]);
    assert_eq!(d.kernel(0).unwrap().calls, 4);
    assert_eq!(d.kernel(0).unwrap().state, 1.);
    d.set_bypassed(false);
    assert_eq!(d.process_sample(0, 7.), Ok(15.));
    // In-place on a fresh duplicator: the same whole-call admission, no clearing.
    let mut d = MonoDuplicator::<f64, _, _>::new(controls(None), 3, 8, factory)
        .ok()
        .unwrap();
    let mut rows = vec![
        vec![1., 2., 3., 99.],
        vec![10., 20., 30., 99.],
        vec![100., 200., 300., 99.],
    ];
    assert_eq!(
        d.process_in_place(&mut views(&mut rows), 3),
        Err(ProcessError::NotPrepared)
    );
    d.prepare(config(3)).unwrap();
    let original = rows.clone();
    assert_eq!(
        d.process_in_place(&mut views(&mut rows), 9),
        Err(ProcessError::TooManyFrames {
            frames: 9,
            maximum: 8
        })
    );
    assert_eq!(
        d.process_in_place(&mut views(&mut rows[..2]), 3),
        Err(ProcessError::LayoutMismatch)
    );
    rows[2].truncate(2);
    let truncated = rows.clone();
    assert_eq!(
        d.process_in_place(&mut views(&mut rows), 3),
        Err(ProcessError::LayoutMismatch)
    );
    assert_eq!(rows, truncated);
    assert_eq!(rows[0], original[0]);
    assert_eq!(rows[1], original[1]);
    for i in 0..3 {
        assert_eq!(d.kernel(i).unwrap().calls, 0);
    }
    rows[2] = original[2].clone();
    d.process_in_place(&mut views(&mut rows), 3).unwrap();
    assert_eq!(rows[0], [2., 5., 8., 99.]);
    assert_eq!(rows[1], [20., 50., 80., 99.]);
    assert_eq!(rows[2], [200., 500., 800., 99.]);
    d.process_in_place(&mut views(&mut rows), 0).unwrap();
    assert_eq!(rows[0], [2., 5., 8., 99.]);
    d.set_bypassed(true);
    d.process_in_place(&mut views(&mut rows), 4).unwrap();
    assert_eq!(rows[0], [2., 5., 8., 99.]);
    assert_eq!(d.kernel(0).unwrap().calls, 3);
}
#[derive(Clone, Copy)]
struct Probe {
    latency: u32,
    tail: Tail,
}
impl SharedMonoKernel<f64, ()> for Probe {
    fn prepare(&mut self, _: f64, _: usize, _: &()) -> Result<(), PrepareError> {
        Ok(())
    }
    fn process_sample(&mut self, x: f64, _: &()) -> f64 {
        x
    }
    fn reset(&mut self, _: &()) {}
    fn deactivate(&mut self) {}
    fn latency_samples(&self) -> u32 {
        self.latency
    }
    fn tail(&self) -> Tail {
        self.tail
    }
}
#[test]
fn latency_and_tail_are_maxima_not_sums_or_positional_choices() {
    let probe = |latency, tail| Probe { latency, tail };
    let cases = [
        // Maximum is in the middle: not first, not last and not the sum (16).
        (
            vec![
                probe(3, Tail::Samples(5)),
                probe(9, Tail::Samples(40)),
                probe(4, Tail::None),
            ],
            9,
            Tail::Samples(40),
        ),
        (
            vec![
                probe(9, Tail::Samples(40)),
                probe(3, Tail::None),
                probe(4, Tail::Samples(5)),
            ],
            9,
            Tail::Samples(40),
        ),
        (
            vec![probe(0, Tail::None), probe(0, Tail::None)],
            0,
            Tail::None,
        ),
        (
            vec![
                probe(1, Tail::Samples(3)),
                probe(2, Tail::Infinite),
                probe(3, Tail::Samples(100)),
            ],
            3,
            Tail::Infinite,
        ),
        (
            vec![probe(0, Tail::Infinite), probe(0, Tail::Samples(3))],
            0,
            Tail::Infinite,
        ),
        (
            vec![probe(0, Tail::None), probe(0, Tail::Samples(7))],
            0,
            Tail::Samples(7),
        ),
        (
            vec![probe(0, Tail::Samples(7)), probe(0, Tail::None)],
            0,
            Tail::Samples(7),
        ),
    ];
    for (kernels, latency, tail) in cases {
        let channels = kernels.len();
        let mut d = MonoDuplicator::<f64, (), Probe>::new((), channels, 8, move |i, _| {
            Ok::<_, ()>(kernels[i])
        })
        .ok()
        .unwrap();
        // Declarations are reported only while prepared.
        assert_eq!(d.latency_samples(), 0);
        assert_eq!(d.tail(), Tail::None);
        d.prepare(config(channels)).unwrap();
        assert_eq!(d.latency_samples(), latency);
        assert_eq!(d.tail(), tail);
        // Bypass freezes histories but keeps the declarations.
        d.set_bypassed(true);
        assert_eq!(d.latency_samples(), latency);
        assert_eq!(d.tail(), tail);
        d.deactivate();
        assert_eq!(d.latency_samples(), 0);
        assert_eq!(d.tail(), Tail::None);
    }
}
#[test]
fn drop_with_active_prepared_kernels_deactivates_each_once_in_reverse() {
    for channels in [1, 3, 64] {
        let c = controls(None);
        let trace = c.trace.clone();
        let drops = c.drops.clone();
        let mut d = MonoDuplicator::<f32, _, _>::new(c, channels, 8, factory)
            .ok()
            .unwrap();
        d.prepare(config(channels)).unwrap();
        assert!((0..channels).all(|i| d.kernel(i).unwrap().active));
        assert!(ids(&trace, "deactivate").is_empty());
        // No explicit deactivate: Drop must release the active kernels itself.
        drop(d);
        assert_eq!(
            ids(&trace, "deactivate"),
            (0..channels).rev().collect::<Vec<_>>()
        );
        assert_eq!(drops.load(Ordering::SeqCst), channels + 1);
    }
}
#[test]
fn reprepare_and_drop_deactivate_each_active_period_exactly_once() {
    let c = controls(None);
    let trace = c.trace.clone();
    let mut d = MonoDuplicator::<f64, _, _>::new(c, 3, 8, factory)
        .ok()
        .unwrap();
    d.prepare(config(3)).unwrap();
    d.prepare(config(3)).unwrap();
    assert_eq!(ids(&trace, "deactivate"), [2, 1, 0]);
    d.deactivate();
    assert_eq!(ids(&trace, "deactivate"), [2, 1, 0, 2, 1, 0]);
    d.deactivate();
    drop(d);
    assert_eq!(ids(&trace, "deactivate").len(), 6);
}
#[test]
fn failed_prepare_dropped_without_explicit_deactivate_has_no_second_deactivation() {
    for failed in [0, 1, 3] {
        let c = controls(Some(failed));
        let trace = c.trace.clone();
        let drops = c.drops.clone();
        let mut d = MonoDuplicator::<f64, _, _>::new(c, 4, 8, factory)
            .ok()
            .unwrap();
        assert_eq!(
            d.prepare(config(4)),
            Err(PrepareError::InvalidSmoothingTime)
        );
        drop(d);
        // A repeated deactivation would trip the kernel's own `assert!(active)`.
        assert_eq!(
            ids(&trace, "deactivate"),
            (0..failed).rev().collect::<Vec<_>>()
        );
        assert_eq!(drops.load(Ordering::SeqCst), 5);
    }
}
#[test]
fn kernel_prepare_panic_leaves_prefix_active_and_drop_releases_it_once() {
    for (channels, failed) in [(1, 0), (2, 1), (5, 0), (5, 3), (64, 63)] {
        // The duplicator outlives the caught panic.
        let mut c = controls(None);
        c.panic_prepare = Some(failed);
        let trace = c.trace.clone();
        let drops = c.drops.clone();
        let mut d = MonoDuplicator::<f32, _, _>::new(c, channels, 8, factory)
            .ok()
            .unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = d.prepare(config(channels));
        }));
        assert!(result.is_err());
        // Successfully prepared prefix is still active, the panicking kernel and the
        // suffix are not, and nothing is deactivated until the owner lets go.
        for i in 0..channels {
            assert_eq!(d.kernel(i).unwrap().active, i < failed);
        }
        assert!(ids(&trace, "deactivate").is_empty());
        assert_eq!(ids(&trace, "prepare"), (0..=failed).collect::<Vec<_>>());
        let input = vec![1.; channels];
        let mut out = vec![0.; channels];
        assert_eq!(
            d.process_frame(&input, &mut out),
            Err(ProcessError::NotPrepared)
        );
        drop(d);
        assert_eq!(
            ids(&trace, "deactivate"),
            (0..failed).rev().collect::<Vec<_>>()
        );
        assert_eq!(drops.load(Ordering::SeqCst), channels + 1);
        // The duplicator is dropped by the unwind itself.
        let mut c = controls(None);
        c.panic_prepare = Some(failed);
        let trace = c.trace.clone();
        let drops = c.drops.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut d = MonoDuplicator::<f64, _, _>::new(c, channels, 8, factory)
                .ok()
                .unwrap();
            let _ = d.prepare(config(channels));
        }));
        assert!(result.is_err());
        assert_eq!(
            ids(&trace, "deactivate"),
            (0..failed).rev().collect::<Vec<_>>()
        );
        assert_eq!(drops.load(Ordering::SeqCst), channels + 1);
    }
}
#[test]
fn refused_prepare_invalidates_but_keeps_kernel_histories_for_the_next_prepare() {
    let c = controls(None);
    let trace = c.trace.clone();
    let mut d = MonoDuplicator::<f64, _, _>::new(c, 2, 8, factory)
        .ok()
        .unwrap();
    d.prepare(config(2)).unwrap();
    let mut out = [0.; 2];
    d.process_frame(&[1., 10.], &mut out).unwrap();
    assert_eq!(d.prepare(config(1)), Err(PrepareError::UnsupportedLayout));
    assert_eq!(d.prepare_failure(), None);
    // Refusal deactivated the active kernels once, in reverse, and left no config.
    assert_eq!(ids(&trace, "deactivate"), [1, 0]);
    assert_eq!(
        d.process_frame(&[2., 20.], &mut out),
        Err(ProcessError::NotPrepared)
    );
    let oversized = ProcessConfig::new(48000., 9, 0, AudioLayout::new(2, 2).unwrap()).unwrap();
    assert_eq!(
        d.prepare(oversized),
        Err(PrepareError::InvalidMaximumFrames)
    );
    assert_eq!(ids(&trace, "deactivate"), [1, 0]);
    assert_eq!(d.kernel(0).unwrap().state, 1.);
    assert_eq!(d.kernel(1).unwrap().state, 10.);
    // These test kernels keep their history across deactivation; the framework adds
    // no reset, so reprepare continues from it.
    d.prepare(config(2)).unwrap();
    d.process_frame(&[2., 20.], &mut out).unwrap();
    assert_eq!(out, [5., 50.]);
}
#[test]
fn prepare_failure_record_is_cleared_only_by_the_next_prepare() {
    let mut d = MonoDuplicator::<f32, _, _>::new(controls(Some(1)), 3, 8, factory)
        .ok()
        .unwrap();
    let expected = Some((1, PrepareError::InvalidSmoothingTime));
    assert_eq!(
        d.prepare(config(3)),
        Err(PrepareError::InvalidSmoothingTime)
    );
    assert_eq!(d.prepare_failure(), expected);
    d.deactivate();
    assert_eq!(d.prepare_failure(), expected);
    d.shared_controls_mut_stopped().fail = None;
    assert_eq!(d.prepare_failure(), expected);
    d.prepare(config(3)).unwrap();
    assert_eq!(d.prepare_failure(), None);
}
