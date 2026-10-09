use std::sync::{
    Arc,
    atomic::{AtomicU32, AtomicUsize, Ordering},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingError {
    InvalidConfiguration,
    PartialFrame,
    WrongQueue,
}
#[repr(align(64))]
struct Position(AtomicUsize);
struct Ring {
    channels: usize,
    capacity: usize,
    samples: Vec<AtomicU32>,
    head: Position,
    tail: Position,
}
/// Unique producer. Only one thread may push; teardown follows callback stop.
pub struct AudioProducer {
    ring: Arc<Ring>,
}
/// Unique consumer. Only one thread may pop; teardown follows callback stop.
pub struct AudioConsumer {
    ring: Arc<Ring>,
}
/// Prepare an SPSC interleaved f32 ring, channels 1..64 and power-of-two capacity
/// 1..65536 frames. Frame counters wrap safely because capacity is a power of two.
/// Endpoints cannot be cloned; transfers use atomic publication, no locks/I/O.
/// Dropping endpoints can deallocate and belongs after audio callbacks stop.
pub fn audio_ring(
    channels: usize,
    frames: usize,
) -> Result<(AudioProducer, AudioConsumer), RingError> {
    if !(1..=64).contains(&channels) || !frames.is_power_of_two() || frames > 65536 {
        return Err(RingError::InvalidConfiguration);
    }
    let ring = Arc::new(Ring {
        channels,
        capacity: frames,
        samples: (0..frames * channels).map(|_| AtomicU32::new(0)).collect(),
        head: Position(AtomicUsize::new(0)),
        tail: Position(AtomicUsize::new(0)),
    });
    Ok((
        AudioProducer {
            ring: Arc::clone(&ring),
        },
        AudioConsumer { ring },
    ))
}
impl AudioProducer {
    pub fn channels(&self) -> usize {
        self.ring.channels
    }
    pub fn capacity_frames(&self) -> usize {
        self.ring.capacity
    }
    pub(crate) fn owns(&self, consumer: &AudioConsumer) -> bool {
        Arc::ptr_eq(&self.ring, &consumer.ring)
    }
    /// Discard queued frames while exclusively borrowing both matching endpoints.
    /// Stop and join endpoint users before reclaiming these borrows for a seek.
    /// A mismatched consumer rejects without modifying either queue.
    pub fn clear(&mut self, consumer: &mut AudioConsumer) -> Result<(), RingError> {
        if !self.owns(consumer) {
            return Err(RingError::WrongQueue);
        }
        let head = self.ring.head.0.load(Ordering::Relaxed);
        self.ring.tail.0.store(head, Ordering::Release);
        Ok(())
    }
    /// Publish a whole-frame prefix that fits. Return accepted frame count;
    /// caller retries or explicitly drops the remaining suffix. Malformed input
    /// fails before publication. No overflow allocation or implicit blocking.
    pub fn push(&mut self, input: &[f32]) -> Result<usize, RingError> {
        let r = &self.ring;
        if !input.len().is_multiple_of(r.channels) {
            return Err(RingError::PartialFrame);
        }
        let head = r.head.0.load(Ordering::Relaxed);
        let tail = r.tail.0.load(Ordering::Acquire);
        let frames = (input.len() / r.channels).min(r.capacity - head.wrapping_sub(tail));
        for f in 0..frames {
            let base = (head.wrapping_add(f) & (r.capacity - 1)) * r.channels;
            for c in 0..r.channels {
                r.samples[base + c].store(input[f * r.channels + c].to_bits(), Ordering::Relaxed);
            }
        }
        r.head.0.store(head.wrapping_add(frames), Ordering::Release);
        Ok(frames)
    }
}
impl AudioConsumer {
    pub fn channels(&self) -> usize {
        self.ring.channels
    }
    pub fn capacity_frames(&self) -> usize {
        self.ring.capacity
    }
    /// Snapshot available frames. A concurrent producer can publish more later.
    pub fn queued_frames(&self) -> usize {
        let tail = self.ring.tail.0.load(Ordering::Relaxed);
        self.ring.head.0.load(Ordering::Acquire).wrapping_sub(tail)
    }
    /// Read a whole-frame prefix. Return frame count; unfilled suffix is untouched
    /// so playback callers choose their own underflow silence policy.
    pub fn pop(&mut self, output: &mut [f32]) -> Result<usize, RingError> {
        let r = &self.ring;
        if !output.len().is_multiple_of(r.channels) {
            return Err(RingError::PartialFrame);
        }
        let tail = r.tail.0.load(Ordering::Relaxed);
        let head = r.head.0.load(Ordering::Acquire);
        let frames = (output.len() / r.channels).min(head.wrapping_sub(tail));
        for f in 0..frames {
            let base = (tail.wrapping_add(f) & (r.capacity - 1)) * r.channels;
            for c in 0..r.channels {
                output[f * r.channels + c] =
                    f32::from_bits(r.samples[base + c].load(Ordering::Relaxed));
            }
        }
        r.tail.0.store(tail.wrapping_add(frames), Ordering::Release);
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counters_wrap_with_whole_frame_order() {
        let (mut producer, mut consumer) = audio_ring(2, 4).unwrap();
        producer
            .ring
            .head
            .0
            .store(usize::MAX - 1, Ordering::Relaxed);
        producer
            .ring
            .tail
            .0
            .store(usize::MAX - 1, Ordering::Relaxed);
        assert_eq!(producer.push(&[1., -1., 2., -2., 3., -3.]).unwrap(), 3);
        let mut output = [0.; 6];
        assert_eq!(consumer.pop(&mut output).unwrap(), 3);
        assert_eq!(output, [1., -1., 2., -2., 3., -3.]);
        assert_eq!(consumer.pop(&mut output).unwrap(), 0);
    }
}
