//! Tests for the self-orchestrating event bus.
//!
//! Expected public API:
//! - EventBus::{new, add_event, delete_event, tick}
//! - EventSlot::once(Box<dyn ScriptedEvent>)
//! - ScriptedEvent::{execute, is_ready, is_dead, as_any}
//!
//! These tests assume `is_ready(&mut self, ...)`, and that actions issued during
//! `tick()` are applied at the beginning of the following `tick()`.

use std::any::Any;

use bevy::{app::App, ecs::world::World, prelude::Resource};

use crate::{EventBus, EventBusPlugin, EventSlot, ScriptedEvent};

#[derive(Resource, Default)]
struct TestState {
    log: Vec<&'static str>,
    iteration: u32,
}

fn test_world() -> World {
    let mut world = World::new();
    world.insert_resource(TestState::default());
    world
}

fn log(world: &World) -> &[&'static str] {
    &world.resource::<TestState>().log
}

fn once<T: ScriptedEvent>(event: T) -> EventSlot {
    EventSlot::once(Box::new(event))
}

// A basic event that immediately executes and records its label.
struct Record(&'static str);

impl ScriptedEvent for Record {
    fn execute(&self, _bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push(self.0);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Another concrete type, to check that deletion is type-specific.
struct OtherRecord;

impl ScriptedEvent for OtherRecord {
    fn execute(&self, _bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push("other");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct ReadyAtIteration(u32);

impl ScriptedEvent for ReadyAtIteration {
    fn is_ready(&mut self, _bus: &EventBus, world: &World) -> bool {
        world.resource::<TestState>().iteration >= self.0
    }

    fn execute(&self, _bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push("ready");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct ReadyAfterChecks {
    checks: usize,
    required: usize,
}

impl ScriptedEvent for ReadyAfterChecks {
    fn is_ready(&mut self, _bus: &EventBus, _world: &World) -> bool {
        self.checks += 1;
        self.checks >= self.required
    }

    fn execute(&self, _bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push("counted");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct AlreadyDead;

impl ScriptedEvent for AlreadyDead {
    fn is_dead(&self, _bus: &EventBus, _world: &World) -> bool {
        true
    }

    fn is_ready(&mut self, _bus: &EventBus, _world: &World) -> bool {
        panic!("is_ready must not run for a dead event");
    }

    fn execute(&self, _bus: &mut EventBus, _world: &mut World) {
        panic!("a dead event must never execute");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct SpawnNext;

impl ScriptedEvent for SpawnNext {
    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push("first");
        bus.add_event(once(Record("second")));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Used to ensure an event cannot recursively execute itself in one tick.
struct SpawnSelf;

impl ScriptedEvent for SpawnSelf {
    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push("self");
        bus.add_event(once(SpawnSelf));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct Victim;

impl ScriptedEvent for Victim {
    fn is_ready(&mut self, _bus: &EventBus, world: &World) -> bool {
        world.resource::<TestState>().iteration >= 1
    }

    fn execute(&self, _bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push("victim");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct CancelVictim;

impl ScriptedEvent for CancelVictim {
    fn execute(&self, bus: &mut EventBus, _world: &mut World) {
        bus.delete_event::<Victim>();
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[test]
fn once_runs_exactly_once() {
    let mut world = test_world();
    let mut bus = EventBus::new();
    bus.add_event(once(Record("once")));

    bus.tick(&mut world);
    bus.tick(&mut world);
    bus.tick(&mut world);

    assert_eq!(log(&world), &["once"]);
}

#[test]
fn event_waits_for_world_condition() {
    let mut world = test_world();
    let mut bus = EventBus::new();
    bus.add_event(once(ReadyAtIteration(2)));

    bus.tick(&mut world);
    assert!(log(&world).is_empty());

    world.resource_mut::<TestState>().iteration = 2;
    bus.tick(&mut world);
    assert_eq!(log(&world), &["ready"]);

    bus.tick(&mut world);
    assert_eq!(log(&world), &["ready"]);
}

#[test]
fn is_ready_can_mutate_its_own_state() {
    let mut world = test_world();
    let mut bus = EventBus::new();
    bus.add_event(once(ReadyAfterChecks {
        checks: 0,
        required: 3,
    }));

    bus.tick(&mut world);
    bus.tick(&mut world);
    assert!(log(&world).is_empty());

    bus.tick(&mut world);
    assert_eq!(log(&world), &["counted"]);
}

#[test]
fn dead_event_is_removed_before_readiness_and_execution() {
    let mut world = test_world();
    let mut bus = EventBus::new();
    bus.add_event(once(AlreadyDead));

    bus.tick(&mut world);
    bus.tick(&mut world);

    assert!(log(&world).is_empty());
}

#[test]
fn event_spawned_during_execute_runs_next_tick() {
    let mut world = test_world();
    let mut bus = EventBus::new();
    bus.add_event(once(SpawnNext));

    bus.tick(&mut world);
    assert_eq!(log(&world), &["first"]);

    bus.tick(&mut world);
    assert_eq!(log(&world), &["first", "second"]);
}

#[test]
fn self_rescheduling_event_runs_only_once_per_tick() {
    let mut world = test_world();
    let mut bus = EventBus::new();
    bus.add_event(once(SpawnSelf));

    for expected in 1..=3 {
        bus.tick(&mut world);
        assert_eq!(log(&world).len(), expected);
    }
}

#[test]
fn delete_removes_all_matching_events_but_not_other_types() {
    let mut world = test_world();
    let mut bus = EventBus::new();
    bus.add_event(once(Record("a")));
    bus.add_event(once(Record("b")));
    bus.add_event(once(OtherRecord));
    bus.delete_event::<Record>();

    bus.tick(&mut world);
    assert_eq!(log(&world), &["other"]);
}

#[test]
fn queued_actions_are_applied_in_order() {
    let mut world = test_world();
    let mut bus = EventBus::new();

    // Delete existing Records, then add a new one: the new one survives.
    bus.add_event(once(Record("old")));
    bus.delete_event::<Record>();
    bus.add_event(once(Record("new")));

    bus.tick(&mut world);
    assert_eq!(log(&world), &["new"]);
}

#[test]
fn deletion_requested_during_execution_applies_on_next_tick() {
    let mut world = test_world();
    let mut bus = EventBus::new();
    bus.add_event(once(CancelVictim));
    bus.add_event(once(Victim));

    // Victim is not ready, and CancelVictim queues its deletion.
    bus.tick(&mut world);
    assert!(log(&world).is_empty());

    // Victim would be ready now, but queued deletion must win.
    world.resource_mut::<TestState>().iteration = 1;
    bus.tick(&mut world);
    assert!(log(&world).is_empty());
}

#[test]
fn plugin_ticks_bus_on_app_update() {
    let mut app = App::new();
    app.add_plugins(EventBusPlugin)
        .insert_resource(TestState::default());

    app.world_mut()
        .resource_mut::<EventBus>()
        .add_event(once(Record("plugin")));

    app.update();
    assert_eq!(log(app.world()), &["plugin"]);

    app.update();
    assert_eq!(log(app.world()), &["plugin"]);
}
