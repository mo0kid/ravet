use super::*;
use ravet_foundation::{AudioBuffer, AudioLayout, Transport};
struct Child {
    called: usize,
    id: usize,
}
impl Processor<f64> for Child {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        self.called += 1;
        Ok(())
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, f64>) -> ProcessStatus {
        self.called += 1;
        b.audio.output_mut(0).unwrap().fill(42.);
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.called += 1;
    }
    fn deactivate(&mut self) {
        self.called += 1;
    }
}
#[test]
fn constructor_refusal_returns_exact_uncalled_owner_and_error() {
    let result = DenormalProcessor::new_using(
        Child { called: 0, id: 987 },
        DenormalMode::FlushBoth,
        || Err(DenormalError::UnsupportedControlMask),
    );
    match result {
        Err(failure) => {
            assert_eq!(failure.processor.id, 987);
            assert_eq!(failure.processor.called, 0);
            assert_eq!(failure.error, DenormalError::UnsupportedControlMask);
        }
        Ok(_) => panic!("synthetic refusal accepted"),
    }
}
#[test]
fn complete_preflight_precedes_refusal_and_poison_retries_never_call_child() {
    let mut adapter = DenormalProcessor::new_using(
        Child { called: 0, id: 876 },
        DenormalMode::FlushBoth,
        || Ok(()),
    )
    .ok()
    .unwrap();
    let config = ProcessConfig::new(48000., 3, 0, AudioLayout::new(1, 1).unwrap()).unwrap();
    adapter.prepare(config).unwrap();
    let input = [1., 2., 3., 4.];
    let inputs = [&input[..]];
    let mut output = [99.; 6];
    {
        let mut outputs = [&mut output[..]];
        let audio = AudioBuffer::separate(&inputs, &mut outputs, 4).unwrap();
        let mut block = ProcessBlock::new(audio, &[], Transport::default());
        assert_eq!(
            adapter.process_using(&mut block, || panic!(
                "preflight must precede native admission"
            )),
            ProcessStatus::Fault(ProcessError::TooManyFrames {
                frames: 4,
                maximum: 3
            })
        );
    }
    assert_eq!(adapter.processor.called, 1);
    assert_eq!(adapter.first_native_error(), None);
    assert_eq!(output, [0., 0., 0., 0., 99., 99.]);
    {
        let mut outputs = [&mut output[..]];
        let audio = AudioBuffer::separate(&inputs, &mut outputs, 3).unwrap();
        let mut block = ProcessBlock::new(audio, &[], Transport::default());
        assert_eq!(
            adapter.process_using(&mut block, || Err(
                DenormalError::UnsupportedAlternateHandling
            )),
            ProcessStatus::Fault(ProcessError::BackendFailure)
        );
        assert_eq!(
            adapter.process_using(&mut block, || panic!(
                "poison retry must not enter native scope"
            )),
            ProcessStatus::Fault(ProcessError::BackendFailure)
        );
    }
    assert_eq!(adapter.processor.called, 1);
    assert_eq!(
        adapter.first_native_error(),
        Some(DenormalError::UnsupportedAlternateHandling)
    );
    assert_eq!(output[4..], [99., 99.]);
    adapter.reset();
    assert_eq!(adapter.first_native_error(), None);
    assert_eq!(adapter.processor.called, 2);
    adapter.deactivate();
    let child = adapter.into_inner().ok().unwrap();
    assert_eq!(child.id, 876);
    assert_eq!(child.called, 3);
}

#[test]
fn late_event_rejection_precedes_native_probe_and_preserves_child_history() {
    use ravet_foundation::{Event, EventData, ParameterId};
    let mut adapter = DenormalProcessor::new_using(
        Child { called: 0, id: 654 },
        DenormalMode::FlushBoth,
        || Ok(()),
    )
    .ok()
    .unwrap();
    let config = ProcessConfig::new(48000., 3, 2, AudioLayout::new(1, 1).unwrap()).unwrap();
    adapter.prepare(config).unwrap();
    let events = [
        Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: ParameterId(0),
                value: 1.,
            },
        },
        Event {
            frame_offset: 2,
            data: EventData::ParameterValue {
                id: ParameterId(0),
                value: f32::NAN,
            },
        },
    ];
    let input = [1., 2., 3.];
    let inputs = [&input[..]];
    let mut output = [99.; 5];
    let mut outputs = [&mut output[..]];
    let audio = AudioBuffer::separate(&inputs, &mut outputs, 3).unwrap();
    let mut block = ProcessBlock::new(audio, &events, Transport::default());
    assert!(matches!(
        adapter.process_using(&mut block, || panic!(
            "late event must reject before hardware"
        )),
        ProcessStatus::Fault(ProcessError::Events(_))
    ));
    assert_eq!(adapter.processor.called, 1);
    assert_eq!(adapter.first_native_error(), None);
    assert_eq!(output, [0., 0., 0., 99., 99.]);
}
