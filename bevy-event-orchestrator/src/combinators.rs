//! Readiness gates that compose without changing event execution or scheduling.

use bevy::{ecs::world::World, prelude::Time};

use crate::{EventBus, ScriptedEvent};

/// Delays inner readiness checks until a game-time deadline.
///
/// Created with [`ScriptedEventExt::after`]. The timer starts on this wrapper's
/// first readiness check, not at construction or registration. The inner event's
/// death check remains active during the delay. Once elapsed, the delay stays
/// elapsed: repeatable slots use their own cooldown for subsequent attempts.
///
/// This wrapper retains its own concrete type and default type name. For example,
/// deleting `E` does not delete `After<E>`; use the wrapper type or an event ID.
pub struct After<E> {
    inner: E,
    delay: f64,
    at: Option<f64>,
}

impl<E: ScriptedEvent> ScriptedEvent for After<E> {
    fn is_ready(&mut self, bus: &EventBus, world: &World) -> bool {
        let now = world.resource::<Time>().elapsed_secs_f64();
        let at = *self.at.get_or_insert(now + self.delay);
        now >= at && self.inner.is_ready(bus, world)
    }

    fn is_dead(&self, bus: &EventBus, world: &World) -> bool {
        self.inner.is_dead(bus, world)
    }

    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        self.inner.execute(bus, world);
    }
}

/// Adds a predicate before the inner event's readiness check.
///
/// Created with [`ScriptedEventExt::when`]. A false predicate skips inner
/// readiness, but never skips the inner death check. The predicate is evaluated
/// again on every eligible check; a previous true result is not latched.
///
/// The closure type is part of this wrapper's concrete identity. Prefer event
/// IDs for cancellation when using anonymous closures. The default name belongs
/// to the wrapper, not the inner event.
pub struct When<E, F> {
    inner: E,
    predicate: F,
}

impl<E, F> ScriptedEvent for When<E, F>
where
    E: ScriptedEvent,
    F: Fn(&EventBus, &World) -> bool + Send + Sync + 'static,
{
    fn is_ready(&mut self, bus: &EventBus, world: &World) -> bool {
        (self.predicate)(bus, world) && self.inner.is_ready(bus, world)
    }

    fn is_dead(&self, bus: &EventBus, world: &World) -> bool {
        self.inner.is_dead(bus, world)
    }

    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        self.inner.execute(bus, world);
    }
}

/// Fluent readiness gates for every concrete [`ScriptedEvent`].
///
/// Import this trait to call `.after(seconds)` and `.when(predicate)`.
/// The last gate added is the outermost and is checked first:
/// `event.after(5.0).when(condition)` starts its timer only once the condition
/// allows a check; `event.when(condition).after(5.0)` starts the timer before
/// checking the condition. A started timer is neither paused nor reset when an
/// outer condition becomes false. Both gates preserve inner readiness and death.
pub trait ScriptedEventExt: ScriptedEvent + Sized {
    /// Adds an initial delay measured using Bevy's [`Time`] resource.
    ///
    /// Zero allows inner readiness on the first check. Delay does not restart
    /// after execution, and elapsed time follows the supplied game clock.
    ///
    /// # Panics
    /// Panics if `seconds` is negative, infinite, or NaN. Readiness checks require
    /// a [`Time`] resource, as does [`EventBus::tick`].
    fn after(self, seconds: f64) -> After<Self> {
        assert!(
            seconds.is_finite() && seconds >= 0.0,
            "event delay must be finite and nonnegative"
        );
        After {
            inner: self,
            delay: seconds,
            at: None,
        }
    }

    /// Requires an additional condition before checking inner readiness.
    ///
    /// Predicates receive read-only access to the bus and world. Captured values
    /// must satisfy `Send + Sync + 'static`; use `move` to capture owned data.
    fn when<F>(self, predicate: F) -> When<Self, F>
    where
        F: Fn(&EventBus, &World) -> bool + Send + Sync + 'static,
    {
        When {
            inner: self,
            predicate,
        }
    }
}

impl<E: ScriptedEvent> ScriptedEventExt for E {}
