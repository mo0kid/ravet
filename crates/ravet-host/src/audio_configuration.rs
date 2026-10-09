//! Owned inactive port snapshots and thread-correct optional timing queries.
use clack_extensions::{
    audio_ports::{AudioPortFlags, AudioPortInfoBuffer, PluginAudioPorts},
    latency::PluginLatency,
    tail::{PluginTail, TailLength},
};
use clack_host::{
    host::HostHandlers,
    plugin::{PluginAudioProcessorHandle, PluginInstance},
    utils::ClapId,
};
use std::{collections::BTreeSet, ffi::CString};
/// Port inspection fails atomically; failed refresh preserves the old snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedAudioError {
    Active,
    Capacity,
    Metadata,
}
/// Byte-preserving owned port information; no plugin pointer is retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostedAudioPort {
    pub id: ClapId,
    pub name: Vec<u8>,
    pub channels: u32,
    pub flags: AudioPortFlags,
    pub port_type: Option<CString>,
    pub in_place_pair: Option<ClapId>,
}
/// Inactive control-thread snapshot, bounded to nine ports per direction and
/// 1..64 channels per port. IDs are unique within each direction; main ports
/// are optional but can only occur at index zero. Port type strings are opaque,
/// at most 256 bytes; names at most 256 bytes. No layout/configuration negotiation
/// or portable main/aux pairing is inferred from directional enumeration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostedAudioConfiguration {
    pub inputs: Vec<HostedAudioPort>,
    pub outputs: Vec<HostedAudioPort>,
}
impl HostedAudioConfiguration {
    /// Missing audio-ports extension means no audio ports, as specified by CLAP.
    /// Plugin code and allocation run only outside callbacks, while inactive.
    pub fn inspect<H: HostHandlers>(
        instance: &mut PluginInstance<H>,
    ) -> Result<Self, HostedAudioError> {
        if instance.inactive_plugin_handle().is_none() {
            return Err(HostedAudioError::Active);
        }
        let handle = instance.plugin_handle();
        let Some(extension) = handle.get_extension::<PluginAudioPorts>() else {
            return Ok(Self::default());
        };
        let mut directions = [Vec::new(), Vec::new()];
        for (direction, ports) in directions.iter_mut().enumerate() {
            let input = direction == 0;
            let count = extension.count(&handle, input);
            if count > 9 {
                return Err(HostedAudioError::Capacity);
            }
            let mut ids = BTreeSet::new();
            let mut buffer = AudioPortInfoBuffer::new();
            for index in 0..count {
                let info = extension
                    .get(&handle, index, input, &mut buffer)
                    .ok_or(HostedAudioError::Metadata)?;
                if !ids.insert(info.id.get())
                    || !(1..=64).contains(&info.channel_count)
                    || info.name.len() > 256
                    || (index != 0 && info.flags.contains(AudioPortFlags::IS_MAIN))
                    || (info.flags.contains(AudioPortFlags::PREFERS_64BITS)
                        && !info.flags.contains(AudioPortFlags::SUPPORTS_64BITS))
                    || info
                        .port_type
                        .is_some_and(|kind| kind.0.to_bytes().len() > 256)
                {
                    return Err(HostedAudioError::Metadata);
                }
                ports.push(HostedAudioPort {
                    id: info.id,
                    name: info.name.to_vec(),
                    channels: info.channel_count,
                    flags: info.flags,
                    port_type: info.port_type.map(|kind| kind.0.to_owned()),
                    in_place_pair: info.in_place_pair,
                });
            }
        }
        for (direction, ports) in directions.iter().enumerate() {
            for port in ports {
                if let Some(pair) = port.in_place_pair {
                    let opposite = directions[1 - direction]
                        .iter()
                        .find(|other| other.id == pair)
                        .ok_or(HostedAudioError::Metadata)?;
                    if opposite.channels != port.channels {
                        return Err(HostedAudioError::Metadata);
                    }
                }
            }
        }
        let [inputs, outputs] = directions;
        Ok(Self { inputs, outputs })
    }
    /// Replace only after complete inspection validates. The caller selects the
    /// instance explicitly; this snapshot does not cache extensions or identity.
    pub fn refresh<H: HostHandlers>(
        &mut self,
        instance: &mut PluginInstance<H>,
    ) -> Result<(), HostedAudioError> {
        *self = Self::inspect(instance)?;
        Ok(())
    }
}
/// Query optional reported latency on the main thread, in samples. Missing
/// extension stays None, rather than inventing a zero-latency declaration.
/// Latency can change during activation; consumers re-query before graph routing.
pub fn hosted_latency<H: HostHandlers>(instance: &mut PluginInstance<H>) -> Option<u32> {
    let handle = instance.plugin_handle();
    handle
        .get_extension::<PluginLatency>()
        .map(|extension| extension.get(&handle))
}
/// Query optional tail on the audio owner thread, with a live Clack processor
/// handle. This does not allocate in RAVET; plugin callback behavior is its contract.
/// Missing extension is None; native raw lengths >= i32::MAX decode as Infinite.
pub fn hosted_tail(handle: &PluginAudioProcessorHandle<'_>) -> Option<TailLength> {
    handle
        .get_extension::<PluginTail>()
        .map(|extension| extension.get(handle))
}
