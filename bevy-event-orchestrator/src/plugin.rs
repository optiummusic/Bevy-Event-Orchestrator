use crate::EventBus;
use bevy::prelude::*;

/// Initializes an [`EventBus`] and ticks it in [`Update`].
///
/// Add Bevy's `MinimalPlugins` or `DefaultPlugins` first to provide game time.
/// An existing bus resource (for example, a seeded one) is preserved.
pub struct EventBusPlugin;

/// Orders systems before or after the event bus tick in [`Update`].
/// Systems placed before this set can queue events for the same update.
/// Systems placed after it observe event effects and queue work for the next update.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventBusSet;

impl Plugin for EventBusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EventBus>()
            .add_systems(Update, tick_event_bus.in_set(EventBusSet));
    }
}

fn tick_event_bus(world: &mut World) {
    world.resource_scope(|world, mut bus: Mut<EventBus>| {
        bus.tick(world);
    });
}
