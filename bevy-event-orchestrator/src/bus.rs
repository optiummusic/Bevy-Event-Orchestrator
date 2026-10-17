//! The event bus type.

use std::any::TypeId;

use bevy::ecs::{resource::Resource, world::World};
use rand::{rngs::StdRng, SeedableRng, RngExt};
use crate::{EventSlot, ScriptedEvent};

enum EventBusActions {
    Add(EventSlot),
    Delete(TypeId),
}

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
    pub fn with_rng(rng: StdRng) -> Self {
        Self {
            events: Vec::new(),
            action: Vec::new(),
            rng,
        }
    }
    pub fn new() -> Self {
        Self::with_rng(rand::make_rng())
    }

    pub fn with_seed(seed: u64) -> Self {
        Self::with_rng(StdRng::seed_from_u64(seed))
    }

    pub fn add_event(&mut self, slot: EventSlot) {
        self.action.push(EventBusActions::Add(slot));
    }

    fn add_event_inner(&mut self, slot: EventSlot) {
        self.events.push(slot);
    }

    pub fn delete_event<T: ScriptedEvent>(&mut self) {
        self.action.push(
            EventBusActions::Delete(TypeId::of::<T>())
        );
    }

    fn delete_event_inner(&mut self, target: TypeId) {
        self.events.retain(|slot| {
            slot.event
                .as_ref()
                .is_none_or(|event| {
                    event.as_any().type_id() != target
                })
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
                },
                EventBusActions::Delete(t) => {
                    self.delete_event_inner(t);
                },
            }
        }
    }

    #[inline]
    pub fn tick(&mut self, world: &mut World) {
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

            if ev.is_ready(self, world) {
                self.events.swap_remove(i);
                ev.execute(self, world);
            } else {
                self.events[i].event = Some(ev);
                i += 1;
            }
        }
    }
}