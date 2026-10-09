//! Fixed global/scoped native parameter mapping into portable bounded events.
use crate::{HostedGraphParameterMapping, HostedParameterError, HostedParameters};
use clack_extensions::params::{ParamInfoFlags, PluginParams};
use clack_host::{
    events::{
        UnknownEvent,
        event_types::{
            ParamGestureBeginEvent, ParamGestureEndEvent, ParamModEvent, ParamValueEvent,
        },
        io::{OutputEventBuffer, TryPushError},
    },
    host::HostHandlers,
    plugin::PluginInstance,
};
use ravet_foundation::{Event, EventData, ParameterGesturePhase};
struct Parameter {
    mapping: HostedGraphParameterMapping,
    minimum: f64,
    maximum: f64,
    stepped: bool,
    readonly: bool,
    modulatable: bool,
    flags: ParamInfoFlags,
}
/// Output events referring to originating note input endpoints, not output ports.
#[derive(Clone, Copy, Default)]
pub(crate) struct InputEndpointOutputProfile {
    pub scoped_parameters: Option<u16>,
    pub completions: Option<u16>,
}
#[derive(Default)]
pub(crate) struct ParameterOutput {
    parameters: Vec<Parameter>,
    note_output: Option<crate::graph_note_output::OutputProfile>,
    gesture_output: bool,
    modulation_output: bool,
    scoped_port: Option<u16>,
    completion_port: Option<u16>,
    pub(crate) extension: Option<PluginParams>,
    maximum: usize,
    frames: usize,
    pub(crate) events: Vec<Event>,
    pub(crate) failed: bool,
    pub(crate) filtered: u64,
    pub(crate) rejected: u64,
}
impl ParameterOutput {
    pub(crate) fn prepare<H: HostHandlers>(
        instance: &mut PluginInstance<H>,
        mappings: Option<Vec<HostedGraphParameterMapping>>,
        note_output: Option<crate::graph_note_output::OutputProfile>,
        gesture_output: bool,
        modulation_output: bool,
        capacity: usize,
        input_endpoints: InputEndpointOutputProfile,
    ) -> Result<Self, HostedParameterError> {
        if mappings
            .as_ref()
            .is_some_and(|mappings| mappings.is_empty() || mappings.len() > 4096)
            || (mappings.is_none()
                && note_output.is_none()
                && input_endpoints.completions.is_none())
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
            parameters.push(Parameter {
                mapping,
                minimum: native.minimum,
                maximum: native.maximum,
                stepped: native.flags.contains(ParamInfoFlags::IS_STEPPED),
                readonly: native.flags.contains(ParamInfoFlags::IS_READONLY),
                modulatable: native.flags.contains(ParamInfoFlags::IS_MODULATABLE),
                flags: native.flags,
            });
        }
        drop(catalogue);
        parameters.sort_by_key(|parameter| parameter.mapping.native_id.get());
        let extension = instance.plugin_handle().get_extension::<PluginParams>();
        if !parameters.is_empty() && extension.is_none() {
            return Err(HostedParameterError::Unavailable);
        }
        Ok(Self {
            parameters,
            extension,
            note_output,
            gesture_output,
            modulation_output,
            scoped_port: input_endpoints.scoped_parameters,
            completion_port: input_endpoints.completions,
            maximum: capacity,
            events: Vec::with_capacity(capacity),
            ..Self::default()
        })
    }
    fn scope(
        &self,
        id: Option<clack_host::utils::ClapId>,
        raw_port: i16,
        channel: i16,
        key: i16,
        note_id: i32,
    ) -> Result<Option<(&Parameter, ravet_foundation::NoteTarget)>, ()> {
        let Some(port) = self.scoped_port else {
            return Ok(None);
        };
        if raw_port != port as i16 {
            return Ok(None);
        }
        let Some(id) = id else {
            return Ok(None);
        };
        let Ok(index) = self
            .parameters
            .binary_search_by_key(&id.get(), |p| p.mapping.native_id.get())
        else {
            return Ok(None);
        };
        let target =
            crate::graph_note_expression::decode_target(port, raw_port, channel, key, note_id)?
                .ok_or(())?;
        Ok(Some((&self.parameters[index], target)))
    }
    fn scoped_modulation(&self, event: &ParamModEvent) -> Result<Option<EventData>, ()> {
        let raw = event.as_raw();
        let Some((parameter, target)) = self.scope(
            event.param_id(),
            raw.port_index,
            raw.channel,
            raw.key,
            raw.note_id,
        )?
        else {
            return Ok(None);
        };
        if parameter.readonly
            || !crate::graph_parameter_scope::supports(parameter.flags, target, true)
            || !raw.amount.is_finite()
        {
            return Err(());
        }
        Ok(Some(EventData::ParameterModulationScoped {
            id: parameter.mapping.graph_id,
            target,
            amount: raw.amount,
        }))
    }
    fn scoped_value(&self, event: &ParamValueEvent) -> Result<Option<EventData>, ()> {
        let raw = event.as_raw();
        let Some((parameter, target)) = self.scope(
            event.param_id(),
            raw.port_index,
            raw.channel,
            raw.key,
            raw.note_id,
        )?
        else {
            return Ok(None);
        };
        if parameter.readonly
            || !crate::graph_parameter_scope::supports(parameter.flags, target, false)
            || !raw.value.is_finite()
            || !(parameter.minimum..=parameter.maximum).contains(&raw.value)
            || (parameter.stepped && raw.value.fract() != 0.)
        {
            return Err(());
        }
        Ok(Some(EventData::ParameterValueScoped {
            id: parameter.mapping.graph_id,
            target,
            value: raw.value,
        }))
    }
    fn gesture(
        &self,
        id: Option<clack_host::utils::ClapId>,
        phase: ParameterGesturePhase,
    ) -> Result<Option<EventData>, ()> {
        if !self.gesture_output {
            return Ok(None);
        }
        let Some(id) = id else {
            return Ok(None);
        };
        let Ok(index) = self
            .parameters
            .binary_search_by_key(&id.get(), |parameter| parameter.mapping.native_id.get())
        else {
            return Ok(None);
        };
        let parameter = &self.parameters[index];
        if parameter.readonly {
            return Err(());
        }
        Ok(Some(EventData::ParameterGesture {
            id: parameter.mapping.graph_id,
            phase,
        }))
    }
    pub(crate) fn begin(&mut self, frames: usize) {
        self.frames = frames;
        self.events.clear();
        self.failed = false;
        self.filtered = 0;
        self.rejected = 0;
    }
    fn reject(&mut self) -> Result<(), TryPushError> {
        self.failed = true;
        self.rejected = self.rejected.saturating_add(1);
        Err(TryPushError::new())
    }
    fn filter(&mut self) -> Result<(), TryPushError> {
        self.filtered = self.filtered.saturating_add(1);
        Ok(())
    }
}
impl OutputEventBuffer for ParameterOutput {
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        if self.maximum == 0 {
            return Err(TryPushError::new());
        }
        let gesture = if let Some(begin) = event.as_event::<ParamGestureBeginEvent>() {
            Some(self.gesture(begin.param_id(), ParameterGesturePhase::Begin))
        } else {
            event
                .as_event::<ParamGestureEndEvent>()
                .map(|end| self.gesture(end.param_id(), ParameterGesturePhase::End))
        };
        let completion = event.as_event::<clack_host::events::event_types::NoteEndEvent>();
        let data = if let Some(end) = completion {
            let Some(port) = self.completion_port else {
                return self.filter();
            };
            let raw = end.as_raw();
            let target = match crate::graph_note_expression::decode_target(
                port,
                raw.port_index,
                raw.channel,
                raw.key,
                raw.note_id,
            ) {
                Ok(Some(target)) => target,
                Ok(None) => return self.filter(),
                Err(()) => return self.reject(),
            };
            if self.frames == 0 {
                return self.reject();
            }
            EventData::NoteEnd { target }
        } else if let Some(gesture) = gesture {
            match gesture {
                Ok(Some(data)) => data,
                Ok(None) => return self.filter(),
                Err(()) => return self.reject(),
            }
        } else if let Some(modulation) = event.as_event::<ParamModEvent>() {
            let raw = modulation.as_raw();
            if (raw.port_index, raw.channel, raw.key, raw.note_id) != (-1, -1, -1, -1) {
                match self.scoped_modulation(modulation) {
                    Ok(Some(data)) => data,
                    Ok(None) => return self.filter(),
                    Err(()) => return self.reject(),
                }
            } else {
                if !self.modulation_output {
                    return self.filter();
                }
                let Some(id) = modulation.param_id() else {
                    return self.filter();
                };
                let Ok(index) = self
                    .parameters
                    .binary_search_by_key(&id.get(), |parameter| parameter.mapping.native_id.get())
                else {
                    return self.filter();
                };
                let parameter = &self.parameters[index];
                let amount = modulation.amount();
                let narrowed = amount as f32;
                if parameter.readonly
                    || !parameter.modulatable
                    || !amount.is_finite()
                    || !narrowed.is_finite()
                {
                    return self.reject();
                }
                EventData::ParameterModulation {
                    id: parameter.mapping.graph_id,
                    amount: narrowed,
                }
            }
        } else if let Some(change) = event.as_event::<ParamValueEvent>() {
            let Some(id) = change.param_id() else {
                return self.filter();
            };
            let Ok(index) = self
                .parameters
                .binary_search_by_key(&id.get(), |parameter| parameter.mapping.native_id.get())
            else {
                return self.filter();
            };
            let raw = change.as_raw();
            if (raw.port_index, raw.channel, raw.key, raw.note_id) != (-1, -1, -1, -1) {
                match self.scoped_value(change) {
                    Ok(Some(data)) => data,
                    Ok(None) => return self.filter(),
                    Err(()) => return self.reject(),
                }
            } else {
                let parameter = &self.parameters[index];
                let value = change.value();
                let narrowed = value as f32;
                if !value.is_finite()
                    || !(parameter.minimum..=parameter.maximum).contains(&value)
                    || !narrowed.is_finite()
                    || !(parameter.minimum..=parameter.maximum).contains(&f64::from(narrowed))
                    || (parameter.stepped && (value.fract() != 0. || narrowed.fract() != 0.))
                {
                    return self.reject();
                }
                EventData::ParameterValue {
                    id: parameter.mapping.graph_id,
                    value: narrowed,
                }
            }
        } else {
            let Some(route) = self.note_output else {
                return self.filter();
            };
            let data = match crate::graph_note_output::decode_with_profile(
                route.route,
                event,
                route.identities,
                route.expressions,
                route.scoped,
            ) {
                Ok(Some(data)) => data,
                Ok(None) => return self.filter(),
                Err(()) => return self.reject(),
            };
            // Parameter flush must never deliver performance output.
            if self.frames == 0 {
                return self.reject();
            }
            data
        };
        // CLAP completion timestamps are ignored. Preserve native delivery order
        // using the previous retained carrier offset, without moving the timed
        // event cursor or inventing a voice termination instant.
        let offset = if completion.is_some() {
            self.events.last().map_or(0, |event| event.frame_offset)
        } else {
            event.header().time() as usize
        };
        if (self.frames == 0 && offset != 0)
            || (self.frames > 0 && offset >= self.frames)
            || self
                .events
                .last()
                .is_some_and(|previous| previous.frame_offset > offset)
            || self.events.len() == self.maximum
        {
            return self.reject();
        }
        self.events.push(Event {
            frame_offset: offset,
            data,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clack_host::events::{Pckn, event_types::NoteEndEvent};
    #[test]
    fn completion_scope_validates_raw_fields_and_ignores_timestamp_velocity() {
        let mut output = ParameterOutput {
            maximum: 4,
            completion_port: Some(1),
            events: Vec::with_capacity(4),
            ..Default::default()
        };
        let native = NoteEndEvent::new(u32::MAX, Pckn::new(1u16, 15u16, 127u16, i32::MAX as u32));
        let mut raw = *native.as_raw();
        raw.velocity = f64::NAN;
        output.begin(8);
        output
            .try_push(NoteEndEvent::from_raw(&raw).as_ref())
            .unwrap();
        assert_eq!(output.events[0].frame_offset, 0);
        assert_eq!(
            output.events[0].data,
            EventData::NoteEnd {
                target: ravet_foundation::NoteTarget {
                    channel: Some(15),
                    key: Some(127),
                    id: Some(ravet_foundation::NoteId(i32::MAX as u32))
                }
            }
        );
        let mut invalid = [raw; 6];
        invalid[0].channel = -2;
        invalid[1].channel = 16;
        invalid[2].key = -2;
        invalid[3].key = 128;
        invalid[4].note_id = -2;
        invalid[5].note_id = i32::MIN;
        let counts = allocation_counter::measure(|| {
            for mut raw in invalid {
                output.begin(8);
                assert!(
                    output
                        .try_push(NoteEndEvent::from_raw(&raw).as_ref())
                        .is_err()
                );
                assert!(output.failed);
                raw.port_index = -1;
                output.begin(8);
                output
                    .try_push(NoteEndEvent::from_raw(&raw).as_ref())
                    .unwrap();
                assert!(output.events.is_empty());
                assert!(!output.failed);
            }
            raw.channel = -1;
            raw.key = -1;
            raw.note_id = -1;
            output.begin(8);
            output
                .try_push(NoteEndEvent::from_raw(&raw).as_ref())
                .unwrap();
            assert_eq!(
                output.events[0].data,
                EventData::NoteEnd {
                    target: ravet_foundation::NoteTarget::default()
                }
            );
            output.begin(0);
            assert!(
                output
                    .try_push(NoteEndEvent::from_raw(&raw).as_ref())
                    .is_err()
            );
        });
        assert_eq!(counts.count_total, 0);
    }
}
