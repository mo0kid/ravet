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
            let header = match self.queue.peek().ok()? {
                Item::Header(header) => *header,
                _ => unreachable!("private fixed batch header"),
            };
            let chunk = self
                .queue
                .read_chunk(self.config.event_capacity + 1)
                .expect("atomic complete batch");
            let (first, second) = chunk.as_slices();
            self.events.clear();
            for item in first.iter().chain(second).skip(1).take(header.count) {
                let Item::Event(event) = item else {
                    unreachable!("private fixed event payload")
                };
                self.events.push(*event);
            }
            chunk.commit_all();
            self.pending = Some(header);
        }
        Some(GraphEventBatch { consumer: self })
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
}
impl GraphEventBatch<'_> {
    pub fn tag(&self) -> u64 {
        self.consumer.pending.expect("retained batch").tag
    }
    pub fn transport(&self) -> Transport {
        self.consumer.pending.expect("retained batch").transport
    }
    pub fn events(&self) -> &[Event] {
        &self.consumer.events
    }
    pub fn acknowledge(self) {
        self.consumer.pending = None;
        self.consumer.events.clear();
    }
}
