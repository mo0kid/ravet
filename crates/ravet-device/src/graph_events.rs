//! Fixed complete event batches for explicitly scheduled graph workers/control.
use ravet_foundation::{Event, EventData, EventError, Transport};
use rtrb::{Consumer, Producer, RingBuffer};

/// Fixed rendering quantum and whole-batch queue budget. No clock is inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphEventQueueConfig {
    pub block_frames: usize,
    pub event_capacity: usize,
    pub queued_batches: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphEventQueueError {
    Configuration,
    Events(EventError),
    Full,
    Disconnected,
}
impl std::fmt::Display for GraphEventQueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "graph event queue: {self:?}")
    }
}
impl std::error::Error for GraphEventQueueError {}
#[derive(Clone, Copy)]
struct Header {
    tag: u64,
    transport: Transport,
    count: usize,
}
#[derive(Clone, Copy)]
enum Item {
    Header(Header),
    Event(Event),
    Unused,
}
/// Unique producer, prepared off callbacks. Each publication is atomic as one
/// complete fixed-slot batch; full/invalid input publishes no prefix. Use for
/// either control input or worker output with an explicit consumer policy.
pub struct GraphEventProducer {
    queue: Producer<Item>,
    config: GraphEventQueueConfig,
}
/// Unique consumer with preallocated retained batch storage. A borrowed batch
/// stays pending until explicit acknowledgement; dropping its borrow does not
/// imply delivery or discard. Endpoints/storage drop after owners stop.
pub struct GraphEventConsumer {
    queue: Consumer<Item>,
    config: GraphEventQueueConfig,
    events: Vec<Event>,
    pending: Option<Header>,
}
/// Allocate 1..65536-frame, 0..4096-event, 1..64-batch SPSC storage on control.
/// Each batch uses capacity+1 slots including unused payload; at most 32 MiB of
/// queue values plus one retained event buffer. Publish/read/acknowledge are
/// bounded without allocation/locks/I/O. Tags/context are opaque caller values;
/// no timestamp translation, retiming, retries or native scheduling is inferred.
pub fn graph_event_queue(
    config: GraphEventQueueConfig,
) -> Result<(GraphEventProducer, GraphEventConsumer), GraphEventQueueError> {
    if !(1..=65536).contains(&config.block_frames)
        || config.event_capacity > 4096
        || !(1..=64).contains(&config.queued_batches)
    {
        return Err(GraphEventQueueError::Configuration);
    }
    let slots = (config.event_capacity + 1) * config.queued_batches;
    if slots > 32 * 1024 * 1024 / std::mem::size_of::<Item>() {
        return Err(GraphEventQueueError::Configuration);
    }
    let (producer, consumer) = RingBuffer::new(slots);
    Ok((
        GraphEventProducer {
            queue: producer,
            config,
        },
        GraphEventConsumer {
            queue: consumer,
            config,
            events: Vec::with_capacity(config.event_capacity),
            pending: None,
        },
    ))
}
impl GraphEventProducer {
    /// Preserve order, offsets and every validated MIDI/UMP word. Normalized
    /// notes/parameter values receive the foundation's value checks. Transport
    /// remains opaque; the graph/native profile applies its context validation.
    pub fn try_publish(
        &mut self,
        tag: u64,
        transport: Transport,
        events: &[Event],
    ) -> Result<(), GraphEventQueueError> {
        let invalid = |error| GraphEventQueueError::Events(error);
        if events.len() > self.config.event_capacity {
            return Err(invalid(EventError::TooMany {
                count: events.len(),
                maximum: self.config.event_capacity,
            }));
        }
        let mut previous = 0;
        for (index, event) in events.iter().enumerate() {
            if event.frame_offset >= self.config.block_frames {
                return Err(invalid(EventError::OffsetOutOfRange {
                    index,
                    offset: event.frame_offset,
                    frames: self.config.block_frames,
                }));
            }
            if event.frame_offset < previous {
                return Err(invalid(EventError::Unordered { index }));
            }
            previous = event.frame_offset;
            let valid = match event.data {
                EventData::ParameterValueScoped { target, value, .. } => {
                    target.is_valid() && value.is_finite()
                }
                EventData::ParameterModulationScoped { target, amount, .. } => {
                    target.is_valid() && amount.is_finite()
                }
                EventData::NoteOffScoped { target, velocity } => {
                    target.is_valid() && velocity.is_finite() && (0. ..=1.).contains(&velocity)
                }
                EventData::NoteChoke { target } | EventData::NoteEnd { target } => {
                    target.is_valid()
                }
                EventData::NoteExpression {
                    target,
                    expression,
                    value,
                } => target.is_valid() && expression.is_valid_value(value),
                EventData::ParameterGesture { .. } => true,
                EventData::ParameterValue { value, .. } => value.is_finite(),
                EventData::ParameterModulation { amount, .. } => amount.is_finite(),
                EventData::NoteWithId {
                    channel,
                    key,
                    velocity,
                    ..
                }
                | EventData::NoteOn {
                    channel,
                    key,
                    velocity,
                }
                | EventData::NoteOff {
                    channel,
                    key,
                    velocity,
                } => {
                    channel < 16
                        && key < 128
                        && velocity.is_finite()
                        && (0. ..=1.).contains(&velocity)
                }
                EventData::Midi(_) | EventData::Ump(_) => true,
            };
            if !valid {
                return Err(invalid(EventError::InvalidValue { index }));
            }
        }
        if self.queue.is_abandoned() {
            return Err(GraphEventQueueError::Disconnected);
        }
        let chunk = self
            .queue
            .write_chunk_uninit(self.config.event_capacity + 1)
            .map_err(|_| GraphEventQueueError::Full)?;
        let written = chunk.fill_from_iter(
            std::iter::once(Item::Header(Header {
                tag,
                transport,
                count: events.len(),
            }))
            .chain(events.iter().copied().map(Item::Event))
            .chain(std::iter::repeat_n(
                Item::Unused,
                self.config.event_capacity - events.len(),
            )),
        );
        debug_assert_eq!(written, self.config.event_capacity + 1);
        Ok(())
    }
}
impl GraphEventConsumer {
    /// Return the retained batch, or dequeue one atomically published batch.
    /// No pending batch is replaced; caller acknowledges only after consuming
    /// or explicitly discarding it under its own graph fault/recovery policy.
    pub fn next_batch(&mut self) -> Option<GraphEventBatch<'_>> {
        if self.pending.is_none() {
            // The only producer commits whole fixed-size batches that start with
            // a header and carry `count <= event_capacity` events, so the
            // fallbacks below are unreachable. They report "no batch" instead
            // of panicking on a worker or audio thread.
            let Item::Header(header) = *self.queue.peek().ok()? else {
                return None;
            };
            let chunk = self.queue.read_chunk(self.config.event_capacity + 1).ok()?;
            let (first, second) = chunk.as_slices();
            self.events.clear();
            for item in first.iter().chain(second).skip(1).take(header.count) {
                if let Item::Event(event) = item {
                    // Within the reserved capacity: no reallocation.
                    self.events.push(*event);
                }
            }
            chunk.commit_all();
            self.pending = Some(header);
        }
        let header = self.pending?;
        Some(GraphEventBatch {
            consumer: self,
            header,
        })
    }
    /// Producer teardown does not discard any retained/queued batches.
    pub fn producer_disconnected(&self) -> bool {
        self.queue.is_abandoned()
    }
}
/// Borrowed immutable pending batch. Drop retains it for another owner tick;
/// acknowledge explicitly after graph consumption or deliberate discard. The
/// tag is caller-owned correlation, not a sample/device-clock identity.
pub struct GraphEventBatch<'a> {
    consumer: &'a mut GraphEventConsumer,
    header: Header,
}
impl GraphEventBatch<'_> {
    pub fn tag(&self) -> u64 {
        self.header.tag
    }
    pub fn transport(&self) -> Transport {
        self.header.transport
    }
    pub fn events(&self) -> &[Event] {
        &self.consumer.events
    }
    pub fn acknowledge(self) {
        self.consumer.pending = None;
        self.consumer.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_private_framing_reports_no_batch_instead_of_panicking() {
        let config = GraphEventQueueConfig {
            block_frames: 64,
            event_capacity: 1,
            queued_batches: 2,
        };
        let (mut producer, mut consumer) = graph_event_queue(config).unwrap();
        // Bypass try_publish: an item that is not a header, then a short batch.
        producer.queue.push(Item::Unused).unwrap();
        assert!(consumer.next_batch().is_none());
        let (_, mut consumer) = graph_event_queue(config).unwrap();
        let (mut queue, ring) = RingBuffer::new(4);
        queue
            .push(Item::Header(Header {
                tag: 7,
                transport: Transport::default(),
                count: 1,
            }))
            .unwrap();
        consumer.queue = ring;
        assert!(consumer.next_batch().is_none());
        // A well-formed batch still carries its header through the batch view.
        let (mut producer, mut consumer) = graph_event_queue(config).unwrap();
        producer.try_publish(9, Transport::default(), &[]).unwrap();
        let batch = consumer.next_batch().unwrap();
        assert_eq!(batch.tag(), 9);
        batch.acknowledge();
        assert!(consumer.next_batch().is_none());
    }
}
