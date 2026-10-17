//! The event bus type.

use std::any::TypeId;

use crate::{EventSlot, ScriptedEvent};
use bevy::{
    ecs::{resource::Resource, world::World},
    prelude::Time,
};
use rand::{RngExt, SeedableRng, rngs::StdRng};

enum EventBusActions {
    Add(EventSlot),
    Delete(TypeId),
}

/// Owns scripted events, deferred queue operations, and a shared random generator.
///
/// Events are scanned linearly each tick. Removal may reorder remaining events;
/// insertion order is not an execution-order guarantee. Prefer separate ticks or
/// explicit world conditions for dependencies between events.
#[derive(Resource)]
pub struct EventBus {
    events: Vec<EventSlot>,
    action: Vec<EventBusActions>,
    rng: StdRng,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// Creates an empty bus with an owned random generator.
    pub fn with_rng(rng: StdRng) -> Self {
        Self {
            events: Vec::new(),
            action: Vec::new(),
            rng,
        }
    }
    /// Creates an empty bus with a randomly initialized generator.
    pub fn new() -> Self {
        Self::with_rng(rand::make_rng())
    }

    /// Reproduces results for the same calls within the same RNG algorithm version.
    /// `StdRng` does not guarantee identical sequences across library versions.
    pub fn with_seed(seed: u64) -> Self {
        Self::with_rng(StdRng::seed_from_u64(seed))
    }

    /// Queues an event for insertion at the beginning of the next tick.
    pub fn add_event(&mut self, slot: EventSlot) {
        self.action.push(EventBusActions::Add(slot));
    }

    fn add_event_inner(&mut self, slot: EventSlot) {
        self.events.push(slot);
    }

    /// Queues removal of all events of concrete type `T`, including repeatable ones.
    /// Operations are applied in call order at the start of the next tick.
    pub fn delete_event<T: ScriptedEvent>(&mut self) {
        self.action.push(EventBusActions::Delete(TypeId::of::<T>()));
    }

    fn delete_event_inner(&mut self, target: TypeId) {
        self.events.retain(|slot| {
            slot.event
                .as_ref()
                .is_none_or(|event| event.as_any().type_id() != target)
        });
    }

    #[inline]
    fn check_action(&mut self) {
        if self.action.is_empty() {
            return;
        }

        let actions = std::mem::take(&mut self.action);
        for act in actions {
            match act {
                EventBusActions::Add(event_slot) => {
                    self.add_event_inner(event_slot);
                }
                EventBusActions::Delete(t) => {
                    self.delete_event_inner(t);
                }
            }
        }
    }

    /// Processes events using Bevy game time. Requires a `Time` resource,
    /// normally provided by Bevy's `TimePlugin` (included in the standard plugin groups).
    ///
    /// Each event is checked for death, cooldown, readiness, and then probability.
    /// A repeatable event makes at most one attempt per tick; missed intervals
    /// are not replayed. The first attempt is eligible immediately.
    ///
    /// # Panics
    /// Panics if `world` has no [`Time`] resource. Callback panics propagate;
    /// recovery and recursive calls to this method are not supported.
    #[inline]
    pub fn tick(&mut self, world: &mut World) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        self.check_action();
        let mut i = 0;
        while i < self.events.len() {
            let Some(mut ev) = self.events[i].event.take() else {
                i += 1;
                continue;
            };

            if ev.is_dead(self, world) {
                self.events.swap_remove(i);
                continue;
            }

            let cooldown_elapsed = self.events[i]
                .repeat
                .as_ref()
                .is_none_or(|repeat| repeat.cooldown_elapsed(now));
            if !cooldown_elapsed || !ev.is_ready(self, world) {
                self.events[i].event = Some(ev);
                i += 1;
                continue;
            }

            if let Some(repeat) = self.events[i].repeat.as_mut() {
                if repeat.roll(now, self.rng.random::<f32>()) {
                    ev.execute(self, world);
                }
                self.events[i].event = Some(ev);
                i += 1;
            } else {
                self.events.swap_remove(i);
                ev.execute(self, world);
            }
        }
    }
}
