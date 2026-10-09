//! Explicit event edges, selected filters and combined audio/event ordering.
use crate::{Graph, GraphError, GraphSample, InputSource, NodeId, OutputDestination};
use ravet_foundation::{Event, EventData, EventError, ParameterId, ProcessBlock};

/// Host input/output (according to edge direction), or one stable graph node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventEndpoint {
    Host,
    Node(NodeId),
}
/// Explicit event selection, without protocol conversion or parameter remapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventFilter {
    /// Every supported portable event kind.
    All,
    /// Values, modulation amounts and gesture boundaries for this stable parameter ID.
    Parameter(ParameterId),
    /// Notes, completion notifications, MIDI short messages and UMP, excluding parameters.
    Performance,
}
impl EventFilter {
    pub(super) fn matches(self, event: Event) -> bool {
        match self {
            Self::All => true,
            Self::Parameter(id) => {
                matches!(event.data, EventData::ParameterValueScoped {id:actual,..} | EventData::ParameterModulationScoped {id:actual,..} | EventData::ParameterModulation {id:actual,..} | EventData::ParameterValue { id: actual, .. } | EventData::ParameterGesture {id:actual,..} if actual == id)
            }
            Self::Performance => !matches!(
                event.data,
                EventData::ParameterValueScoped { .. }
                    | EventData::ParameterModulationScoped { .. }
                    | EventData::ParameterModulation { .. }
                    | EventData::ParameterValue { .. }
                    | EventData::ParameterGesture { .. }
            ),
        }
    }
}
/// Directed event connection. Duplicate connections deliberately duplicate events.
/// Equal-time ordering follows connection declaration then source event order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventConnection {
    pub source: EventEndpoint,
    pub destination: EventEndpoint,
    pub filter: EventFilter,
}
#[derive(Clone, Copy)]
pub(super) struct EventRoute {
    pub source: Option<usize>,
    pub destination: Option<usize>,
    pub filter: EventFilter,
}
impl<S: GraphSample> Graph<S> {
    /// Select explicit event routing while inactive, replacing default input
    /// broadcast. Empty routes deliver no events. Combined audio/event cycles
    /// reject before replacing the current routes/order. At most 4096 edges.
    /// Event offsets remain unchanged; audio latency delays do not delay events.
    pub fn set_event_routes(&mut self, connections: &[EventConnection]) -> Result<(), GraphError> {
        if self.config.is_some() {
            return Err(GraphError::Active);
        }
        if connections.len() > 4096 {
            return Err(GraphError::Capacity);
        }
        let index = |endpoint| match endpoint {
            EventEndpoint::Host => Ok(None),
            EventEndpoint::Node(id) => self
                .nodes
                .iter()
                .position(|node| node.definition.id == id)
                .map(Some)
                .ok_or(GraphError::MissingNode(id)),
        };
        let mut routes = Vec::with_capacity(connections.len());
        for connection in connections {
            routes.push(EventRoute {
                source: index(connection.source)?,
                destination: index(connection.destination)?,
                filter: connection.filter,
            });
        }
        let mut edges = Vec::new();
        for route in &self.routes {
            if let (InputSource::Node(source, _), OutputDestination::Node(destination, _)) =
                (route.source, route.destination)
            {
                edges.push((source, destination));
            }
        }
        for route in &routes {
            if let (Some(source), Some(destination)) = (route.source, route.destination) {
                edges.push((source, destination));
            }
        }
        let mut degrees = vec![0usize; self.nodes.len()];
        for &(_, destination) in &edges {
            degrees[destination] += 1;
        }
        let mut done = vec![false; self.nodes.len()];
        let mut order = Vec::with_capacity(self.nodes.len());
        for _ in 0..self.nodes.len() {
            let index = (0..self.nodes.len())
                .find(|&index| !done[index] && degrees[index] == 0)
                .ok_or(GraphError::Cycle)?;
            done[index] = true;
            order.push(index);
            for &(source, destination) in &edges {
                if source == index {
                    degrees[destination] -= 1;
                }
            }
        }
        self.event_routes = Some(routes);
        self.order = order;
        Ok(())
    }
    pub(super) fn collect_node_events(
        &mut self,
        destination: usize,
        host: &[Event],
        maximum: usize,
    ) -> Result<(), (EventRoute, EventError)> {
        self.nodes[destination].input_events.clear();
        for route in self.event_routes.as_ref().unwrap() {
            if route.destination != Some(destination) {
                continue;
            }
            let (source_events, destination_events) = if let Some(source) = route.source {
                let (source_node, destination_node) = if source < destination {
                    let (before, after) = self.nodes.split_at_mut(destination);
                    (&before[source], &mut after[0])
                } else {
                    let (before, after) = self.nodes.split_at_mut(source);
                    (&after[0], &mut before[destination])
                };
                (
                    &source_node.output_events[..source_node.output_event_count],
                    &mut destination_node.input_events,
                )
            } else {
                (host, &mut self.nodes[destination].input_events)
            };
            append_filtered(source_events, destination_events, route.filter, maximum)
                .map_err(|error| (*route, error))?;
        }
        Ok(())
    }
    pub(super) fn collect_host_events(
        &mut self,
        host: &[Event],
        maximum: usize,
    ) -> Result<(), (EventRoute, EventError)> {
        self.host_events.clear();
        for route in self.event_routes.as_ref().unwrap() {
            if route.destination.is_some() {
                continue;
            }
            let source_events = route.source.map_or(host, |source| {
                &self.nodes[source].output_events[..self.nodes[source].output_event_count]
            });
            append_filtered(source_events, &mut self.host_events, route.filter, maximum)
                .map_err(|error| (*route, error))?;
        }
        Ok(())
    }
    pub(super) fn event_fault(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, S>,
        route: EventRoute,
        error: EventError,
    ) -> crate::GraphStatus {
        self.faulted = true;
        block.clear_outputs();
        crate::GraphStatus::EventFault {
            source: route.source.map(|source| self.nodes[source].definition.id),
            destination: route
                .destination
                .map(|destination| self.nodes[destination].definition.id),
            error,
        }
    }
}
pub(super) fn insert(
    events: &mut Vec<Event>,
    maximum: usize,
    event: Event,
) -> Result<(), EventError> {
    if events.len() >= maximum {
        return Err(EventError::TooMany {
            count: events.len().saturating_add(1),
            maximum,
        });
    }
    let position = events.partition_point(|existing| existing.frame_offset <= event.frame_offset);
    events.insert(position, event);
    Ok(())
}

fn append_filtered(
    source: &[Event],
    destination: &mut Vec<Event>,
    filter: EventFilter,
    maximum: usize,
) -> Result<(), EventError> {
    for &event in source {
        if filter.matches(event) {
            insert(destination, maximum, event)?;
        }
    }
    Ok(())
}
