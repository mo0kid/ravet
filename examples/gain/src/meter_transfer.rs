//! Coalesced, one-attempt telemetry publication; readers retain old data on contention.
use crate::GainMeters;
use ravet::foundation::{PublishError, SnapshotMailbox};
use ravet::signal::MeterReading;

/// Separate audio-to-control mailbox. Intermediate blocks may be overwritten.
/// No retries, event queue, GUI scheduling, or peak retention across blocks.
pub struct GainMeterMailbox(SnapshotMailbox<20>);
impl Default for GainMeterMailbox {
    fn default() -> Self {
        Self(SnapshotMailbox::new([0; 20]))
    }
}
impl GainMeterMailbox {
    /// Publish one complete input/output block reading; drop it when busy.
    pub fn try_publish(&self, meters: GainMeters) -> Result<u64, PublishError> {
        let mut words = [0; 20];
        for (index, reading) in meters.input.into_iter().chain(meters.output).enumerate() {
            let start = index * 5;
            words[start..start + 5].copy_from_slice(&[
                reading.peak.to_bits(),
                reading.rms.to_bits(),
                reading.samples as u32,
                (reading.samples >> 32) as u32,
                u32::from(reading.nonfinite),
            ]);
        }
        self.0.try_publish(words)
    }
    /// One coherent read, or None while publication overlaps the read.
    pub fn try_read(&self) -> Option<GainMeters> {
        let snapshot = self.0.try_read()?;
        let readings: [MeterReading; 4] = std::array::from_fn(|index| {
            let start = index * 5;
            MeterReading {
                peak: f32::from_bits(snapshot.words[start]),
                rms: f32::from_bits(snapshot.words[start + 1]),
                samples: snapshot.words[start + 2] as u64
                    | ((snapshot.words[start + 3] as u64) << 32),
                nonfinite: snapshot.words[start + 4] != 0,
            }
        });
        Some(GainMeters {
            input: [readings[0], readings[1]],
            output: [readings[2], readings[3]],
        })
    }
}
