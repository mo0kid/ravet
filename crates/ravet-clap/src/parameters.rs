//! Borrowed global parameter metadata with validated CLAP representation.
use clack_extensions::params::{ParamInfo, ParamInfoFlags};
use clack_plugin::utils::{ClapId, Cookie};
use ravet_foundation::{ParameterId, ParameterSchema};

/// Explicit per-scope native parameter metadata; no translation/DSP is enabled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClapParameterScopes {
    pub note_id: bool,
    pub key: bool,
    pub channel: bool,
    pub port: bool,
}
impl ClapParameterScopes {
    /// Advertise no scoped support; useful in static metadata options.
    pub const NONE: Self = Self {
        note_id: false,
        key: false,
        channel: false,
        port: false,
    };
    /// Advertise support at all four scopes; implementation remains consumer-owned.
    pub const ALL: Self = Self {
        note_id: true,
        key: true,
        channel: true,
        port: true,
    };
}
/// Consumer-owned CLAP presentation and processing policy, in schema order.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClapParameterOptions<'a> {
    /// UTF-8 module path, at most 1023 bytes, with no embedded NUL.
    pub module: &'a str,
    /// Advertise the sole binary 0/1 stepped host bypass parameter.
    pub bypass: bool,
    /// Active changes affect output and must be delivered through process.
    pub requires_process: bool,
    /// Advertise global physical-unit modulation; the processor must implement it.
    /// This does not enable input translation or per-note flags automatically.
    pub global_modulation: bool,
    /// Advertised scoped automation support, independently of global automation.
    pub scoped_automation: ClapParameterScopes,
    /// Advertised scoped modulation, independently of global modulation.
    pub scoped_modulation: ClapParameterScopes,
}
/// Unsupported metadata or physical representation rejects registration atomically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClapParameterError {
    Capacity,
    Metadata(ParameterId),
    SteppedRange(ParameterId),
    Bypass(ParameterId),
}
impl std::fmt::Display for ClapParameterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CLAP parameter registration: {self:?}")
    }
}
impl std::error::Error for ClapParameterError {}

/// Immutable 0..1024-parameter global CLAP metadata registration.
/// Borrows a validated portable schema and ordered consumer options. Plain values
/// remain physical f32 values promoted exactly to f64; logarithmic editor mappings
/// do not change the wire range. Stepped ranges must be contiguous unit-spaced
/// integers; fractional/coarser discrete schemas need a separate wire mapping.
/// Names are complete UTF-8 up to 255 bytes, modules up to 1023 bytes, without NUL.
/// CLAP's invalid ID is rejected. At most one bypass, with binary range, is allowed.
/// Explicit per-scope flags advertise consumer support. No cookies, value storage, formatting, flush, host
/// rescans or lifecycle policy are synthesized. Construction belongs to preparation;
/// metadata reads do not allocate. Products still implement PluginMainThreadParams.
#[derive(Clone, Copy, Debug)]
pub struct ClapParameterRegistration<'a, const N: usize> {
    schema: ParameterSchema<'a, N>,
    options: &'a [ClapParameterOptions<'a>; N],
}
impl<'a, const N: usize> ClapParameterRegistration<'a, N> {
    pub fn new(
        schema: ParameterSchema<'a, N>,
        options: &'a [ClapParameterOptions<'a>; N],
    ) -> Result<Self, ClapParameterError> {
        if N > 1024 {
            return Err(ClapParameterError::Capacity);
        }
        let mut bypass = false;
        for (descriptor, option) in schema.descriptors().iter().zip(options) {
            let id = descriptor.id;
            if id.0 == u32::MAX
                || descriptor.name.len() > 255
                || descriptor.name.contains('\0')
                || option.module.len() > 1023
                || option.module.contains('\0')
            {
                return Err(ClapParameterError::Metadata(id));
            }
            let range = descriptor.range;
            if let Some(intervals) = range.intervals()
                && (range.minimum().fract() != 0.0
                    || range.maximum().fract() != 0.0
                    || f64::from(range.maximum()) - f64::from(range.minimum())
                        != f64::from(intervals))
            {
                return Err(ClapParameterError::SteppedRange(id));
            }
            if option.bypass {
                if bypass
                    || range.minimum() != 0.0
                    || range.maximum() != 1.0
                    || range.intervals() != Some(1)
                {
                    return Err(ClapParameterError::Bypass(id));
                }
                bypass = true;
            }
        }
        Ok(Self { schema, options })
    }
    pub fn count(&self) -> u32 {
        N as u32
    }
    /// Exact complete metadata; an out-of-range host index returns None.
    pub fn info(&self, index: u32) -> Option<ParamInfo<'a>> {
        let descriptor = self.schema.descriptors().get(index as usize)?;
        let option = self.options[index as usize];
        let mut flags = ParamInfoFlags::empty();
        flags.set(ParamInfoFlags::IS_AUTOMATABLE, descriptor.automatable);
        flags.set(ParamInfoFlags::IS_MODULATABLE, option.global_modulation);
        for (value, flag) in [
            (
                option.scoped_automation.note_id,
                ParamInfoFlags::IS_AUTOMATABLE_PER_NOTE_ID,
            ),
            (
                option.scoped_automation.key,
                ParamInfoFlags::IS_AUTOMATABLE_PER_KEY,
            ),
            (
                option.scoped_automation.channel,
                ParamInfoFlags::IS_AUTOMATABLE_PER_CHANNEL,
            ),
            (
                option.scoped_automation.port,
                ParamInfoFlags::IS_AUTOMATABLE_PER_PORT,
            ),
            (
                option.scoped_modulation.note_id,
                ParamInfoFlags::IS_MODULATABLE_PER_NOTE_ID,
            ),
            (
                option.scoped_modulation.key,
                ParamInfoFlags::IS_MODULATABLE_PER_KEY,
            ),
            (
                option.scoped_modulation.channel,
                ParamInfoFlags::IS_MODULATABLE_PER_CHANNEL,
            ),
            (
                option.scoped_modulation.port,
                ParamInfoFlags::IS_MODULATABLE_PER_PORT,
            ),
        ] {
            flags.set(flag, value);
        }

        flags.set(
            ParamInfoFlags::IS_STEPPED,
            descriptor.range.intervals().is_some(),
        );
        flags.set(ParamInfoFlags::IS_BYPASS, option.bypass);
        flags.set(ParamInfoFlags::REQUIRES_PROCESS, option.requires_process);
        Some(ParamInfo {
            id: ClapId::new(descriptor.id.0),
            flags,
            cookie: Cookie::empty(),
            name: descriptor.name.as_bytes(),
            module: option.module.as_bytes(),
            min_value: f64::from(descriptor.range.minimum()),
            max_value: f64::from(descriptor.range.maximum()),
            default_value: f64::from(descriptor.default),
        })
    }
}
