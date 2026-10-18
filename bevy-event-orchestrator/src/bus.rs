//! The event bus type.

use std::any::TypeId;

use crate::{EventSlot, ScriptedEvent};
use bevy::{
    ecs::{resource::Resource, world::World},
    prelude::Time,
};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Identifies one registration within its originating bus.
///
/// IDs are assigned immediately by [`EventBus::add_event`]. They are not global:
/// separate buses can issue equal IDs. The default zero ID is never assigned.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct EventId(u64);

enum EventBusActions {
    Add(Box<EventSlot>),
    DeleteType(TypeId),
    DeleteId(EventId),
}

/// Owns scripted events, deferred operations, and the seed for per-event RNG streams.
///
/// Events are scanned linearly each tick. Removal may reorder remaining events;
/// insertion order is not an execution-order guarantee. Prefer separate ticks or
/// explicit world conditions for dependencies between events.
#[derive(Resource)]
pub struct EventBus {
    events: Vec<EventSlot>,
    pending: Vec<EventBusActions>,
    seed: u64,
    next_id: u64,
    current_key: Option<u64>,
    child_n: u32,
    root_counts: bevy::platform::collections::HashMap<TypeId, u32>,
}

/// Computes the 64-bit FNV-1a hash used when deriving event stream keys.
/// This is a non-cryptographic hash; collisions are possible.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
fn child_key(parent: u64, n: u32) -> u64 {
    let mut b = [0u8; 12];
    b[..8].copy_from_slice(&parent.to_le_bytes());
    b[8..].copy_from_slice(&n.to_le_bytes());
    fnv1a(&b)
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// Creates an empty bus with a random seed.
    pub fn new() -> Self {
        Self::with_seed(rand::random())
    }

    /// Creates an empty bus with reproducible per-event ChaCha8 streams.
    /// Root keys depend on event names and registration counts per concrete type.
    /// Child keys depend on the parent key and child position within `execute`.
    /// Repeated executions of a parent currently reuse child stream keys.
    pub fn with_seed(seed: u64) -> Self {
        Self {
            events: Vec::new(),
            pending: Vec::new(),
            seed,
            next_id: 1,
            current_key: None,
            child_n: 0,
            root_counts: Default::default(),
        }
    }

    /// Queues an event; it is inserted at the start of the next tick.
    /// The id is valid immediately.
    pub fn add_event(&mut self, mut slot: EventSlot) -> EventId {
        let id = EventId(self.next_id);
        self.next_id += 1;
        slot.id = id;

        slot.key = match (slot.explicit_key, self.current_key) {
            (Some(k), _) => k,
            (None, Some(parent)) => {
                let n = self.child_n;
                self.child_n += 1;
                child_key(parent, n)
            }
            (None, None) => {
                let tid = slot.type_id.expect("slot built without type id");
                let ev = slot.event.as_ref().expect("slot without event");
                let c = self.root_counts.entry(tid).or_insert(0);
                *c += 1;
                child_key(fnv1a(ev.name().as_bytes()), *c)
            }
        };

        if slot.repeat.is_some() {
            let mut rng = ChaCha8Rng::seed_from_u64(self.seed);
            rng.set_stream(slot.key);
            slot.rng = Some(rng);
        }
        self.pending.push(EventBusActions::Add(Box::new(slot)));
        id
    }

    /// Selects deletion by [`DeleteEvent::by_id`] or [`DeleteEvent::by_type`].
    /// Operations are applied in call order at the start of the next tick.
    pub fn delete_event(&mut self) -> DeleteEvent<'_> {
        DeleteEvent { bus: self }
    }
    /// Queries membership after replaying queued additions and deletions.
    /// This does not evaluate readiness, death predicates, or future execution.
    pub fn is_alive(&self) -> IsAliveEvent<'_> {
        IsAliveEvent { bus: self }
    }

    #[inline]
    fn check_action(&mut self) {
        if self.pending.is_empty() {
            return;
        }

        for act in std::mem::take(&mut self.pending) {
            match act {
                EventBusActions::Add(s) => self.events.push(*s),
                EventBusActions::DeleteId(id) => self.events.retain(|s| s.id != id),
                EventBusActions::DeleteType(t) => self.events.retain(|s| s.type_id != Some(t)),
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

            let cooled = self.events[i]
                .repeat
                .as_ref()
                .is_none_or(|r| r.cooldown_elapsed(now));
            if !cooled || !ev.is_ready(self, world) {
                self.events[i].event = Some(ev);
                i += 1;
                continue;
            }

            let key = self.events[i].key;
            if self.events[i].repeat.is_some() {
                let slot = &mut self.events[i];
                let repeat = slot.repeat.as_mut().unwrap();
                // Don't spend a random draw when the outcome is certain.
                let r = if repeat.chance() >= 1.0 {
                    0.0
                } else {
                    slot.rng.as_mut().unwrap().random::<f32>()
                };
                let fire = repeat.roll(now, r);
                if fire {
                    self.run(&*ev, key, world);
                }
                self.events[i].event = Some(ev);
                i += 1;
            } else {
                self.events.swap_remove(i);
                self.run(&*ev, key, world);
            }
        }
    }

    fn run(&mut self, ev: &dyn ScriptedEvent, key: u64, world: &mut World) {
        self.current_key = Some(key);
        self.child_n = 0;
        ev.execute(self, world);
        self.current_key = None;
    }
}

/// A deferred deletion request created by [`EventBus::delete_event`].
/// Call `by_id` or `by_type` to enqueue the operation.
pub struct DeleteEvent<'a> {
    bus: &'a mut EventBus,
}
impl DeleteEvent<'_> {
    /// Queues removal of a single registration. Unknown IDs are harmless.
    pub fn by_id(self, id: EventId) {
        self.bus.pending.push(EventBusActions::DeleteId(id));
    }
    /// Queues removal of all registrations of concrete type `T`.
    pub fn by_type<T: ScriptedEvent>(self) {
        self.bus
            .pending
            .push(EventBusActions::DeleteType(TypeId::of::<T>()));
    }
}

/// A view of event membership including pending operations in queue order.
///
/// A pending deletion is reported immediately, even though the event can still
/// run during the current tick. During `execute`, a one-shot event has already
/// been removed, while a repeatable event is still registered.
pub struct IsAliveEvent<'a> {
    bus: &'a EventBus,
}
impl IsAliveEvent<'_> {
    /// Returns whether this registration survives the pending operations.
    pub fn by_id(self, id: EventId) -> bool {
        let bus = self.bus;
        let mut alive = false;
        let mut ty = None;
        if let Some(s) = bus.events.iter().find(|s| s.id == id) {
            alive = true;
            ty = s.event_type_id();
        }
        for a in &bus.pending {
            match a {
                EventBusActions::Add(s) if s.id == id => {
                    alive = true;
                    ty = s.event_type_id();
                }
                EventBusActions::DeleteId(d) if *d == id => alive = false,
                EventBusActions::DeleteType(t) if ty == Some(*t) => alive = false,
                _ => {}
            }
        }
        alive
    }

    /// Returns whether any registration of `T` survives pending operations.
    /// This query allocates temporary storage for matching IDs.
    pub fn by_type<T: ScriptedEvent>(self) -> bool {
        let bus = self.bus;
        let t = TypeId::of::<T>();
        let mut ids: Vec<EventId> = bus
            .events
            .iter()
            .filter(|s| s.event_type_id() == Some(t))
            .map(|s| s.id)
            .collect();
        for a in &bus.pending {
            match a {
                EventBusActions::Add(s) if s.event_type_id() == Some(t) => ids.push(s.id),
                EventBusActions::DeleteType(d) if *d == t => ids.clear(),
                EventBusActions::DeleteId(d) => ids.retain(|i| i != d),
                _ => {}
            }
        }
        !ids.is_empty()
    }
}
