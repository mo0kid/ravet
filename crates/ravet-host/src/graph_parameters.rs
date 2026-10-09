//! Main-prepared global/scoped parameter identity/range mapping for graph callbacks.
use crate::{HostedParameterError, HostedParameters};
use clack_extensions::params::{ParamInfoFlags, PluginParams};
use clack_host::{
    events::{
        Pckn, UnknownEvent,
        event_types::{
            ParamGestureBeginEvent, ParamGestureEndEvent, ParamModEvent, ParamValueEvent,
        },
        io::InputEventBuffer,
    },
    host::HostHandlers,
    plugin::PluginInstance,
    utils::ClapId,
};
use ravet_foundation::{Event, EventData, EventError, ParameterId, ProcessError};
/// Explicit product-stable graph identity to current native parameter identity.
/// Values retain plain physical units; no normalization/scaling is inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostedGraphParameterMapping {
    pub graph_id: ParameterId,
    pub native_id: ClapId,
}
struct Parameter {
    mapping: HostedGraphParameterMapping,
    minimum: f64,
    maximum: f64,
    stepped: bool,
    modulatable: bool,
    flags: ParamInfoFlags,
}
pub(crate) struct ParameterInput {
    parameters: Vec<Parameter>,
    maximum: usize,
    pub(crate) events: ParameterEvents,
    pub(crate) extension: Option<PluginParams>,
    note_input: Option<crate::graph_note_input::InputProfile>,
    modulation_input: bool,
    gesture_input: bool,
    scoped_input: bool,
}
#[derive(Default)]
pub(crate) struct ParameterEvents(Vec<NativeInput>);
enum NativeInput {
    Parameter(ParamValueEvent),
    Modulation(ParamModEvent),
    Begin(ParamGestureBeginEvent),
    End(ParamGestureEndEvent),
    Note(crate::graph_note_input::NativeNote),
}
// SAFETY: private owned events are constructed only through ParamValueEvent/ParamModEvent::new,
// with empty cookies/owned scalar Pckn; gesture events contain only values; note/MIDI/UMP variants contain only values. No plugin/borrowed pointer is stored or exposed
// mutably. Native reads are scoped to its owner callback, then storage is reused.
#[allow(unsafe_code)]
unsafe impl Send for ParameterEvents {}
impl InputEventBuffer for ParameterEvents {
    fn len(&self) -> u32 {
        self.0.len() as u32
    }
    fn get(&self, index: u32) -> Option<&UnknownEvent> {
        self.0
            .as_slice()
            .get(index as usize)
            .map(|event| match event {
                NativeInput::Parameter(event) => event.as_ref(),
                NativeInput::Modulation(event) => event.as_ref(),
                NativeInput::Begin(event) => event.as_ref(),
                NativeInput::End(event) => event.as_ref(),
                NativeInput::Note(note) => match note {
                    crate::graph_note_input::NativeNote::Expression(event) => event.as_ref(),
                    crate::graph_note_input::NativeNote::Choke(event) => event.as_ref(),
                    crate::graph_note_input::NativeNote::On(event) => event.as_ref(),
                    crate::graph_note_input::NativeNote::Off(event) => event.as_ref(),
                    crate::graph_note_input::NativeNote::Midi(event) => event.as_ref(),
                    crate::graph_note_input::NativeNote::Ump(event) => event.as_ref(),
                },
            })
    }
}
impl ParameterInput {
    pub(crate) fn prepare<H: HostHandlers>(
        instance: &mut PluginInstance<H>,
        mappings: Option<Vec<HostedGraphParameterMapping>>,
        note_input: Option<crate::graph_note_input::InputProfile>,
        capacity: usize,
        modulation_input: bool,
        gesture_input: bool,
        scoped_input: bool,
    ) -> Result<Self, HostedParameterError> {
        if mappings
            .as_ref()
            .is_some_and(|mappings| mappings.is_empty() || mappings.len() > 4096)
            || (mappings.is_none() && note_input.is_none())
            || !(1..=4096).contains(&capacity)
        {
            return Err(HostedParameterError::Capacity);
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut native_ids = std::collections::BTreeSet::new();
        let catalogue = if mappings.is_some() {
            Some(HostedParameters::new(instance, 4096)?)
        } else {
            None
        };
        let mut parameters = Vec::with_capacity(mappings.as_ref().map_or(0, Vec::len));
        for mapping in mappings.into_iter().flatten() {
            if !ids.insert(mapping.graph_id.0) || !native_ids.insert(mapping.native_id.get()) {
                return Err(HostedParameterError::Metadata);
            }
            let native = catalogue
                .as_ref()
                .expect("selected parameter catalogue")
                .parameters()
                .iter()
                .find(|parameter| parameter.id == mapping.native_id)
                .ok_or(HostedParameterError::Unknown)?;
            if native.flags.contains(ParamInfoFlags::IS_READONLY) {
                return Err(HostedParameterError::ReadOnly);
            }
            parameters.push(Parameter {
                mapping,
                minimum: native.minimum,
                maximum: native.maximum,
                stepped: native.flags.contains(ParamInfoFlags::IS_STEPPED),
                modulatable: native.flags.contains(ParamInfoFlags::IS_MODULATABLE),
                flags: native.flags,
            });
        }
        drop(catalogue);
        parameters.sort_by_key(|parameter| parameter.mapping.graph_id.0);
        let extension = instance.plugin_handle().get_extension::<PluginParams>();
        if !parameters.is_empty() && extension.is_none() {
            return Err(HostedParameterError::Unavailable);
        }
        Ok(Self {
            parameters,
            maximum: capacity,
            events: ParameterEvents(Vec::with_capacity(capacity)),
            extension,
            note_input,
            modulation_input,
            gesture_input,
            scoped_input,
        })
    }
    pub(crate) fn encode(&mut self, events: &[Event], frames: usize) -> Result<(), ProcessError> {
        if events.len() > self.maximum {
            return Err(ProcessError::Events(EventError::TooMany {
                count: events.len(),
                maximum: self.maximum,
            }));
        }
        self.events.0.clear();
        for (index, event) in events.iter().enumerate() {
            let invalid = || ProcessError::Events(EventError::InvalidValue { index });
            if let EventData::ParameterValueScoped { id, target, value }
            | EventData::ParameterModulationScoped {
                id,
                target,
                amount: value,
            } = event.data
            {
                if !self.scoped_input || !value.is_finite() {
                    return Err(invalid());
                }
                let route = self.note_input.ok_or_else(invalid)?.route;
                let found = self
                    .parameters
                    .binary_search_by_key(&id.0, |p| p.mapping.graph_id.0)
                    .map_err(|_| invalid())?;
                let parameter = &self.parameters[found];
                let modulation = matches!(event.data, EventData::ParameterModulationScoped { .. });
                if !crate::graph_parameter_scope::supports(parameter.flags, target, modulation)
                    || (!modulation
                        && (!(parameter.minimum..=parameter.maximum).contains(&value)
                            || (parameter.stepped && value.fract() != 0.)))
                {
                    return Err(invalid());
                }
                let pckn =
                    crate::graph_note_expression::encode_target(route.port_index, target, index)?;
                self.events.0.push(if modulation {
                    NativeInput::Modulation(ParamModEvent::new(
                        event.frame_offset as u32,
                        parameter.mapping.native_id,
                        pckn,
                        value,
                    ))
                } else {
                    NativeInput::Parameter(ParamValueEvent::new(
                        event.frame_offset as u32,
                        parameter.mapping.native_id,
                        pckn,
                        value,
                    ))
                });
                continue;
            }
            if let EventData::ParameterGesture { id, phase } = event.data {
                if !self.gesture_input {
                    return Err(invalid());
                }
                let found = self
                    .parameters
                    .binary_search_by_key(&id.0, |parameter| parameter.mapping.graph_id.0)
                    .map_err(|_| invalid())?;
                let native = self.parameters[found].mapping.native_id;
                let event = match phase {
                    ravet_foundation::ParameterGesturePhase::Begin => NativeInput::Begin(
                        ParamGestureBeginEvent::new(event.frame_offset as u32, native),
                    ),
                    ravet_foundation::ParameterGesturePhase::End => NativeInput::End(
                        ParamGestureEndEvent::new(event.frame_offset as u32, native),
                    ),
                };
                self.events.0.push(event);
                continue;
            }
            if let EventData::ParameterModulation { id, amount } = event.data {
                if !self.modulation_input || !amount.is_finite() {
                    return Err(invalid());
                }
                let found = self
                    .parameters
                    .binary_search_by_key(&id.0, |parameter| parameter.mapping.graph_id.0)
                    .map_err(|_| invalid())?;
                let parameter = &self.parameters[found];
                if !parameter.modulatable {
                    return Err(invalid());
                }
                self.events
                    .0
                    .push(NativeInput::Modulation(ParamModEvent::new(
                        event.frame_offset as u32,
                        parameter.mapping.native_id,
                        Pckn::match_all(),
                        f64::from(amount),
                    )));
                continue;
            }
            let EventData::ParameterValue { id, value } = event.data else {
                if frames == 0 {
                    return Err(invalid());
                }
                let route = self.note_input.ok_or_else(invalid)?;
                let event = crate::graph_note_input::encode_with_profile(
                    route.route,
                    event.data,
                    event.frame_offset as u32,
                    index,
                    route.identities,
                    route.expressions,
                    route.scoped,
                )?;
                self.events.0.push(NativeInput::Note(event));
                continue;
            };
            let found = self
                .parameters
                .binary_search_by_key(&id.0, |parameter| parameter.mapping.graph_id.0)
                .map_err(|_| invalid())?;
            let parameter = &self.parameters[found];
            let value = f64::from(value);
            if !value.is_finite()
                || !(parameter.minimum..=parameter.maximum).contains(&value)
                || (parameter.stepped && value.fract() != 0.)
            {
                return Err(invalid());
            }
            self.events
                .0
                .push(NativeInput::Parameter(ParamValueEvent::new(
                    event.frame_offset as u32,
                    parameter.mapping.native_id,
                    Pckn::match_all(),
                    value,
                )));
        }
        Ok(())
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.events.0.is_empty()
    }
}
