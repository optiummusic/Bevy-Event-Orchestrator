//! Event trait

// We use Any for dynamic access.
use bevy::ecs::world::World;

use crate::EventBus;
pub trait ScriptedEvent: std::any::Any + Send + Sync {
    fn execute(&self, bus: &mut EventBus, world: &mut World);

    #[allow(unused_variables)]
    fn is_ready(&mut self, bus: &EventBus, world: &World) -> bool {
        true
    }

    #[allow(unused_variables)]
    fn is_dead(&self, bus: &EventBus, world: &World) -> bool {
        false
    }

    fn as_any(&self) -> &dyn std::any::Any;
}