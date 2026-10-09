use clack_host::events::io::EventBuffer;
use clack_plugin::{
    events::{
        Pckn,
        event_types::{ParamGestureBeginEvent, ParamGestureEndEvent, ParamValueEvent},
        io::InputEvents,
    },
    utils::ClapId,
};
use ravet_clap::{BridgeError, ParameterEvents};
use ravet_foundation::{Event, EventData, ParameterGesturePhase, ParameterId};
fn gesture(offset: usize, phase: ParameterGesturePhase) -> Event {
    Event {
        frame_offset: offset,
        data: EventData::ParameterGesture {
            id: ParameterId(7),
            phase,
        },
    }
}
#[test]
fn raw_boundaries_values_order_flush_and_heap_contract() {
    let mut native = EventBuffer::with_capacity(3);
    native.push(&ParamGestureBeginEvent::new(2, ClapId::new(7)));
    native.push(&ParamValueEvent::new(
        2,
        ClapId::new(7),
        Pckn::match_all(),
        0.5,
    ));
    native.push(&ParamGestureEndEvent::new(6, ClapId::new(7)));
    let mut disabled = ParameterEvents::with_capacity(3).unwrap();
    assert_eq!(
        disabled
            .translate(&InputEvents::from_buffer(&native), 8, false)
            .unwrap()
            .len(),
        1
    );
    let mut enabled = ParameterEvents::with_capacity(3)
        .unwrap()
        .with_parameter_gesture_input(true);
    let counts = allocation_counter::measure(|| {
        for _ in 0..10000 {
            let events = enabled
                .translate(&InputEvents::from_buffer(&native), 8, false)
                .unwrap();
            assert_eq!(events[0], gesture(2, ParameterGesturePhase::Begin));
            assert_eq!(events[2], gesture(6, ParameterGesturePhase::End));
            assert_eq!(
                events[1],
                Event {
                    frame_offset: 2,
                    data: EventData::ParameterValue {
                        id: ParameterId(7),
                        value: 0.5
                    }
                }
            );
            let events = enabled
                .translate(&InputEvents::from_buffer(&native), 0, true)
                .unwrap();
            assert_eq!(events[0], gesture(0, ParameterGesturePhase::Begin));
            assert_eq!(events[2], gesture(0, ParameterGesturePhase::End));
        }
    });
    assert_eq!(counts.count_total, 0);
    // Raw ends do not require a corresponding begin or ownership policy.
    let ends = [ParamGestureEndEvent::new(0, ClapId::new(7)); 2];
    assert_eq!(
        enabled
            .translate(&InputEvents::from_buffer(&ends), 8, false)
            .unwrap(),
        &[gesture(0, ParameterGesturePhase::End); 2]
    );
}
#[test]
fn complete_traversal_capacity_and_timing_reject_even_when_disabled() {
    for enabled in [false, true] {
        let mut translator = ParameterEvents::with_capacity(2)
            .unwrap()
            .with_parameter_gesture_input(enabled);
        for times in [[4, 3], [0, 8]] {
            let native = times.map(|time| ParamGestureBeginEvent::new(time, ClapId::new(7)));
            assert_eq!(
                translator.translate(&InputEvents::from_buffer(&native), 8, false),
                Err(BridgeError::InvalidEvent)
            );
        }
        let native = [ParamGestureEndEvent::new(u32::MAX, ClapId::new(7)); 3];
        assert_eq!(
            translator.translate(&InputEvents::from_buffer(&native), 0, true),
            Err(BridgeError::TooManyEvents)
        );
        let native = [ParamGestureEndEvent::new(u32::MAX, ClapId::new(7))];
        let events = translator
            .translate(&InputEvents::from_buffer(&native), 0, true)
            .unwrap();
        assert_eq!(events.len(), usize::from(enabled));
        if enabled {
            assert_eq!(events[0], gesture(0, ParameterGesturePhase::End));
        }
    }
}
