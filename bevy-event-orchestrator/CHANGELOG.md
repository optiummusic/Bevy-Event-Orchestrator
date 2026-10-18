# Changelog

## 0.1.1 — Unreleased

- Remove the required `as_any()` method; cache concrete event type identity.
- Return `EventId` from registration; support deletion by ID or type through
  `delete_event().by_id(id)` and `delete_event().by_type::<T>()`.
- Add `is_alive()` queries that account for pending queue operations.
- Use per-event ChaCha8 RNG streams derived from root/parent keys.
- Add overridable event names used in root stream keys.
- Remove `EventSlot::default()`; slots must contain an event.
- Update scenario examples and regression tests for handles and stream isolation.

These API and RNG changes break compatibility with the earlier implementation.

## 0.1.0 — Unreleased

- Condition-driven one-shot and repeatable scripted events.
- Game-time cooldowns and per-attempt probability checks.
- Shared seeded RNG for reproducible scenarios within an RNG version.
- Deferred event registration and deletion by concrete type.
- Bevy plugin and system set for update scheduling.
