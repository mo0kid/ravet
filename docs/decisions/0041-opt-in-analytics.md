# 0041 — Default-disabled worker analytics

Date: 9 October 2026. Status: implemented within stated limits.

A reusable analytics mechanism must not choose private product consent policy,
identity or endpoint. Supply an optional worker queue that starts disabled,
accepts validated numeric events only after explicit consumer enable, and erases
pending records on revoke. No module construction/event method can initiate
network activity. Explicit flush supplies a complete bounded JSON batch to a
consumer sender; failure retains it, success clears it. Consumers decide what
HTTP status represents acceptance and whether/when to retry.

Bound event names/metrics/encoded bytes and complete batch bytes/event count.
Newest overflow rejects without removing accepted events. Exclusive borrowing
serializes revoke/flush, but revocation cannot undo accepted remote delivery.
Response loss means retries may duplicate a batch; exactly-once delivery is not
claimed. Methods allocate and sender code can block, so none belong in audio.
Synthetic sender tests cover consent, revocation, no-send idle behavior, bounded
JSON ordering, preservation on errors and malformed/oversized events. No actual
analytics transmission or consumer activation integration is claimed.
