use ravet_embedded::{BlockError, FixedBlockAdapter, PcmSample};
use ravet_foundation::*;

#[derive(Default)]
struct Probe {
    calls: usize,
    fail: bool,
    instrument: bool,
}
impl Processor for Probe {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.calls += 1;
        block.audio.map_stereo_frames(|_, input| {
            if self.instrument {
                [0.25, -0.25]
            } else {
                [input[0] * 2.0, input[1] * 2.0]
            }
        });
        if self.fail {
            ProcessStatus::Fault(ProcessError::BackendFailure)
        } else {
            ProcessStatus::Continue
        }
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
}
fn config(frames: usize, events: usize) -> ProcessConfig {
    ProcessConfig::new(48000.0, frames, events, AudioLayout::STEREO).unwrap()
}
#[test]
fn interleaving_pcm_saturation_and_tails() {
    let mut adapter = FixedBlockAdapter::<_, 2, 2, 8>::new(Probe::default());
    adapter.prepare(config(8, 2)).unwrap();
    let mut output = [7_i16; 8];
    adapter
        .process_interleaved(
            &[16384_i16, -16384, 8192, -8192, 30000, -30000],
            &mut output,
            3,
            &[],
            Transport::default(),
        )
        .unwrap();
    assert_eq!(output, [32767, -32768, 16384, -16384, 32767, -32768, 7, 7]);
    assert_eq!(i16::from_f32(f32::NAN), 0);
    assert_eq!(i16::from_f32(f32::INFINITY), i16::MAX);
    assert_eq!(i32::from_f32(-1.0), i32::MIN);
    assert_eq!(i32::from_f32(1.0), i32::MAX);
    assert_eq!(i32::MIN.to_f32(), -1.0);
}
#[test]
fn malformed_blocks_reject_before_dsp_and_silence_only_active_output() {
    let mut adapter = FixedBlockAdapter::<_, 2, 2, 8>::new(Probe::default());
    adapter.prepare(config(4, 2)).unwrap();
    let input = [1.0; 10];
    let event = |offset| Event {
        frame_offset: offset,
        data: EventData::ParameterValue {
            id: ParameterId(0),
            value: 1.0,
        },
    };
    for events in [
        &[event(1), event(0)][..],
        &[event(3)][..],
        &[event(0), event(0), event(0)][..],
    ] {
        let mut output = [9.0; 8];
        assert!(
            adapter
                .process_interleaved(&input, &mut output, 3, events, Transport::default())
                .is_err()
        );
        assert_eq!(output, [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 9.0, 9.0]);
    }
    let mut output = [9.0; 8];
    assert_eq!(
        adapter.process_interleaved(&input[..1], &mut output, 2, &[], Transport::default()),
        Err(BlockError::InputTooShort)
    );
    assert_eq!(output, [0.0, 0.0, 0.0, 0.0, 9.0, 9.0, 9.0, 9.0]);
    let mut output = [9.0; 3];
    assert_eq!(
        adapter.process_interleaved(&input, &mut output, 2, &[], Transport::default()),
        Err(BlockError::OutputTooShort)
    );
    assert_eq!(output, [0.0; 3]);
    assert!(
        adapter
            .process_interleaved(&input, &mut output, usize::MAX, &[], Transport::default())
            .is_err()
    );
    assert_eq!(adapter.processor().calls, 0);
}
#[test]
fn instruments_zero_frames_failed_prepare_and_backend_faults() {
    let mut adapter = FixedBlockAdapter::<_, 0, 2, 8>::new(Probe {
        instrument: true,
        ..Probe::default()
    });
    let cfg = ProcessConfig::new(48000.0, 8, 2, AudioLayout::STEREO_INSTRUMENT).unwrap();
    adapter.prepare(cfg).unwrap();
    let mut output = [9.0; 6];
    adapter
        .process_interleaved(&[] as &[f32], &mut output, 2, &[], Transport::default())
        .unwrap();
    assert_eq!(output, [0.25, -0.25, 0.25, -0.25, 9.0, 9.0]);
    adapter
        .process_interleaved(&[] as &[f32], &mut output, 0, &[], Transport::default())
        .unwrap();
    assert_eq!(adapter.processor().calls, 2);
    assert!(adapter.prepare(cfg.with_output_events(1)).is_err());
    assert_eq!(
        adapter.process_interleaved(&[] as &[f32], &mut output, 2, &[], Transport::default()),
        Err(BlockError::Process(ProcessError::NotPrepared))
    );
    let mut faulty = FixedBlockAdapter::<_, 2, 2, 8>::new(Probe {
        fail: true,
        ..Probe::default()
    });
    faulty.prepare(config(8, 2)).unwrap();
    assert_eq!(
        faulty.process_interleaved(&[1.0; 4], &mut output, 2, &[], Transport::default()),
        Err(BlockError::Process(ProcessError::BackendFailure))
    );
    assert_eq!(output, [0.0, 0.0, 0.0, 0.0, 9.0, 9.0]);
}
#[test]
fn callback_reset_and_faults_allocate_and_deallocate_nothing() {
    let mut adapter = FixedBlockAdapter::<_, 2, 2, 8>::new(Probe::default());
    adapter.prepare(config(8, 2)).unwrap();
    let input = [1_i16; 16];
    let mut output = [0_i16; 16];
    let counts = allocation_counter::measure(|| {
        for _ in 0..1000 {
            adapter
                .process_interleaved(&input, &mut output, 8, &[], Transport::default())
                .unwrap();
            assert!(
                adapter
                    .process_interleaved(&input[..1], &mut output, 8, &[], Transport::default())
                    .is_err()
            );
            adapter.reset();
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(counts.bytes_current, 0);
}
