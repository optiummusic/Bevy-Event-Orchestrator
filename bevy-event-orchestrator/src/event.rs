//! Event trait

// Any supplies concrete type identity without boilerplate in event implementations.
use bevy::ecs::world::World;

use crate::EventBus;
/// A condition-driven action owned by an [`EventBus`].
///
/// Callbacks run synchronously. During plugin execution the bus is temporarily
/// removed from the world: use the supplied `bus`, not `world.resource::<EventBus>()`.
/// Do not recursively call [`EventBus::tick`] from a callback.
pub trait ScriptedEvent: std::any::Any + Send + Sync {
    /// Executes the action. Queue additions or deletions through `bus`; they take
    /// effect at the start of the next tick. Mutations to `world` are immediate.
    fn execute(&self, bus: &mut EventBus, world: &mut World);

    /// Returns whether an eligible event may attempt execution (default: `true`).
    /// Called after the cooldown check. Returning `false` consumes neither a
    /// random sample nor the cooldown. May update the event's internal state.
    #[allow(unused_variables)]
    fn is_ready(&mut self, bus: &EventBus, world: &World) -> bool {
        true
    }

    /// Permanently removes the event when `true` (default: `false`).
    /// Checked on every tick, including during cooldown, before readiness.
    #[allow(unused_variables)]
    fn is_dead(&self, bus: &EventBus, world: &World) -> bool {
        false
    }

    /// Returns a diagnostic name, defaulting to the concrete Rust type name.
    /// Root-event RNG keys also use this name, so renaming a type or overriding
    /// this method changes its random stream. Use distinct, stable names when
    /// overriding it for different event types.
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}
