//! Owned bounded parameter metadata and explicit inactive edits for live instances.
use clack_extensions::params::{ParamInfoBuffer, ParamInfoFlags, PluginParams};
use clack_host::{
    events::{
        Pckn,
        event_types::ParamValueEvent,
        io::{InputEvents, OutputEvents},
    },
    host::HostHandlers,
    plugin::PluginInstance,
    utils::ClapId,
};
use std::collections::BTreeSet;

/// Parameter discovery or selected-value edit failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedParameterError {
    /// Plugin does not expose the parameter extension.
    Unavailable,
    /// Requested maximum is outside 1..4096 or plugin count exceeds it.
    Capacity,
    /// Missing/duplicate IDs, invalid ranges or invalid returned values.
    Metadata,
    /// Requested stable ID is not in the inspected catalogue.
    Unknown,
    /// This API edits only inactive instances; active edits belong to audio.
    Active,
    /// Plugin marks the selected parameter read-only.
    ReadOnly,
    /// Value is nonfinite, outside the range or nonintegral for a stepped parameter.
    Value,
}
/// Owned plain-value metadata; bytes retain plugin encoding without guessing UTF-8.
/// No opaque cookie or pointer survives metadata inspection.
#[derive(Clone, Debug, PartialEq)]
pub struct HostedParameter {
    pub id: ClapId,
    pub flags: ParamInfoFlags,
    pub name: Vec<u8>,
    pub module: Vec<u8>,
    pub minimum: f64,
    pub maximum: f64,
    pub default: f64,
}
impl HostedParameter {
    fn valid_value(&self, value: f64) -> bool {
        value.is_finite()
            && (self.minimum..=self.maximum).contains(&value)
            && (!self.flags.contains(ParamInfoFlags::IS_STEPPED) || value.fract() == 0.)
    }
}
/// Exclusive main-thread parameter access to a live CLAP instance.
/// Creation/refresh allocate and execute plugin code outside audio callbacks.
/// The borrow prevents cached extensions from outliving or switching instances.
/// No host parameter extension, automatic rescan or automation scheduler is implied.
pub struct HostedParameters<'a, H: HostHandlers> {
    instance: &'a mut PluginInstance<H>,
    extension: PluginParams,
    parameters: Vec<HostedParameter>,
    maximum: usize,
}
impl<'a, H: HostHandlers> HostedParameters<'a, H> {
    /// Inspect at most `maximum` parameters (1..4096), accepting zero parameters.
    pub fn new(
        instance: &'a mut PluginInstance<H>,
        maximum: usize,
    ) -> Result<Self, HostedParameterError> {
        if !(1..=4096).contains(&maximum) {
            return Err(HostedParameterError::Capacity);
        }
        let extension = instance
            .plugin_handle()
            .get_extension::<PluginParams>()
            .ok_or(HostedParameterError::Unavailable)?;
        let mut guard = Self {
            instance,
            extension,
            parameters: Vec::new(),
            maximum,
        };
        guard.refresh()?;
        Ok(guard)
    }
    /// Replace the whole catalogue only after validation succeeds. The consumer
    /// decides when a plugin rescan/state change requires refreshing metadata.
    pub fn refresh(&mut self) -> Result<(), HostedParameterError> {
        let handle = self.instance.plugin_handle();
        let count = self.extension.count(&handle) as usize;
        if count > self.maximum {
            return Err(HostedParameterError::Capacity);
        }
        let mut parameters = Vec::with_capacity(count);
        let mut ids = BTreeSet::new();
        let mut buffer = ParamInfoBuffer::new();
        for index in 0..count {
            let info = self
                .extension
                .get_info(&handle, index as u32, &mut buffer)
                .ok_or(HostedParameterError::Metadata)?;
            if !ids.insert(info.id.get())
                || info.name.len() > 256
                || info.module.len() > 1024
                || !info.min_value.is_finite()
                || !info.max_value.is_finite()
                || info.min_value > info.max_value
            {
                return Err(HostedParameterError::Metadata);
            }
            let parameter = HostedParameter {
                id: info.id,
                flags: info.flags,
                name: info.name.to_vec(),
                module: info.module.to_vec(),
                minimum: info.min_value,
                maximum: info.max_value,
                default: info.default_value,
            };
            if !parameter.valid_value(parameter.default) {
                return Err(HostedParameterError::Metadata);
            }
            parameters.push(parameter);
        }
        self.parameters = parameters;
        Ok(())
    }
    /// Validated owned metadata in current plugin enumeration order.
    pub fn parameters(&self) -> &[HostedParameter] {
        &self.parameters
    }
    /// Read a finite, in-range plain value by stable ID; no normalization/scaling.
    pub fn value(&mut self, id: ClapId) -> Result<f64, HostedParameterError> {
        let parameter = self
            .parameters
            .iter()
            .find(|parameter| parameter.id == id)
            .ok_or(HostedParameterError::Unknown)?;
        let value = self
            .extension
            .get_value(&self.instance.plugin_handle(), id)
            .ok_or(HostedParameterError::Metadata)?;
        if !parameter.valid_value(value) {
            return Err(HostedParameterError::Metadata);
        }
        Ok(value)
    }
    /// Send one zero-time global value to an inactive plugin. Caller supplies the
    /// outgoing queue; edits validate before invoking plugin code. This reports
    /// dispatch, not acknowledgement: query `value` to check resulting state.
    /// Active sample-timed automation belongs to the audio process event stream.
    pub fn set_inactive(
        &mut self,
        id: ClapId,
        value: f64,
        output: &mut OutputEvents<'_>,
    ) -> Result<(), HostedParameterError> {
        let parameter = self
            .parameters
            .iter()
            .find(|parameter| parameter.id == id)
            .ok_or(HostedParameterError::Unknown)?;
        if parameter.flags.contains(ParamInfoFlags::IS_READONLY) {
            return Err(HostedParameterError::ReadOnly);
        }
        if !parameter.valid_value(value) {
            return Err(HostedParameterError::Value);
        }
        let mut handle = self
            .instance
            .inactive_plugin_handle()
            .ok_or(HostedParameterError::Active)?;
        let event = [ParamValueEvent::new(0, id, Pckn::match_all(), value)];
        self.extension
            .flush(&mut handle, &InputEvents::from_buffer(&event), output);
        Ok(())
    }
}
