//! Run with `cargo run --example basic` (no window or renderer required).

use bevy::prelude::*;
use bevy_event_orchestrator::{EventBus, EventBusPlugin, EventSlot, ScriptedEvent};

#[derive(Resource, Default)]
struct Score(u32);

struct AwardPoint;

impl ScriptedEvent for AwardPoint {
    fn execute(&self, _bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<Score>().0 += 1;
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(EventBus::with_seed(42))
        .add_plugins(EventBusPlugin)
        .init_resource::<Score>();

    app.world_mut()
        .resource_mut::<EventBus>()
        .add_event(EventSlot::once(AwardPoint));

    app.update();
    assert_eq!(app.world().resource::<Score>().0, 1);
    println!("Score: {}", app.world().resource::<Score>().0);
}
