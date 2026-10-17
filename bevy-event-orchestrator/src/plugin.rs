use bevy::prelude::*;
use crate::EventBus;

pub struct EventBusPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventBusSet;

impl Plugin for EventBusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EventBus>()
            .add_systems(Update, tick_event_bus.in_set(EventBusSet));
    }
}

fn tick_event_bus(world: &mut World) {
    world.resource_scope(
        |world, mut bus: Mut<EventBus>| {
            bus.tick(world);
        },
    );
}