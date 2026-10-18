//! Tests for the self-orchestrating event bus.
//!
//! Expected public API:
//! - EventBus::{new, add_event, delete_event, tick}
//! - EventSlot::once(impl ScriptedEvent)
//! - ScriptedEvent::{execute, is_ready, is_dead}
//!
//! These tests assume `is_ready(&mut self, ...)`, and that actions issued during
//! `tick()` are applied at the beginning of the following `tick()`.

use std::time::Duration;

use bevy::{
    app::App,
    ecs::world::World,
    prelude::{Resource, Time},
};

use crate::{EventBus, EventBusPlugin, EventSlot, ScriptedEvent};

#[derive(Resource, Default)]
struct TestState {
    log: Vec<&'static str>,
    iteration: u32,
}

fn test_world() -> World {
    let mut world = World::new();
    world.insert_resource(TestState::default());
    world.insert_resource(Time::<()>::default());
    world
}

fn log(world: &World) -> &[&'static str] {
    &world.resource::<TestState>().log
}

fn once<T: ScriptedEvent>(event: T) -> EventSlot {
    EventSlot::once(event)
}

// A basic event that immediately executes and records its label.
struct Record(&'static str);

impl ScriptedEvent for Record {
    fn execute(&self, _bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push(self.0);
    }
}

// Another concrete type, to check that deletion is type-specific.
struct OtherRecord;

impl ScriptedEvent for OtherRecord {
    fn execute(&self, _bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push("other");
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
}

struct SpawnNext;

impl ScriptedEvent for SpawnNext {
    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push("first");
        bus.add_event(once(Record("second")));
    }
}

// Used to ensure an event cannot recursively execute itself in one tick.
struct SpawnSelf;

impl ScriptedEvent for SpawnSelf {
    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push("self");
        bus.add_event(once(SpawnSelf));
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
}

struct CancelVictim;

impl ScriptedEvent for CancelVictim {
    fn execute(&self, bus: &mut EventBus, _world: &mut World) {
        bus.delete_event().by_type::<Victim>();
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
    bus.delete_event().by_type::<Record>();

    bus.tick(&mut world);
    assert_eq!(log(&world), &["other"]);
}

#[test]
fn queued_actions_are_applied_in_order() {
    let mut world = test_world();
    let mut bus = EventBus::new();

    // Delete existing Records, then add a new one: the new one survives.
    bus.add_event(once(Record("old")));
    bus.delete_event().by_type::<Record>();
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
    app.insert_resource(Time::<()>::default())
        .add_plugins(EventBusPlugin)
        .insert_resource(TestState::default());

    app.world_mut()
        .resource_mut::<EventBus>()
        .add_event(once(Record("plugin")));

    app.update();
    assert_eq!(log(app.world()), &["plugin"]);

    app.update();
    assert_eq!(log(app.world()), &["plugin"]);
}

fn advance(world: &mut World, seconds: f64) {
    world
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f64(seconds));
}

fn repeating(event: impl ScriptedEvent, cooldown: f64, chance: f32) -> EventSlot {
    EventSlot::repeatable_chance(event, cooldown, chance)
}

#[test]
fn repeatable_runs_again_only_after_cooldown() {
    let mut world = test_world();
    let mut bus = EventBus::with_seed(42);
    bus.add_event(EventSlot::repeatable(Record("repeat"), 10.0));
    bus.tick(&mut world);
    advance(&mut world, 9.0);
    bus.tick(&mut world);
    assert_eq!(log(&world), &["repeat"]);
    advance(&mut world, 1.0);
    bus.tick(&mut world);
    advance(&mut world, 10.0);
    bus.tick(&mut world);
    assert_eq!(log(&world), &["repeat", "repeat", "repeat"]);
}

#[test]
fn zero_chance_never_executes() {
    let mut world = test_world();
    let mut bus = EventBus::with_seed(42);
    bus.add_event(repeating(Record("never"), 10.0, 0.0));
    for _ in 0..100 {
        bus.tick(&mut world);
        advance(&mut world, 10.0);
    }
    assert!(log(&world).is_empty());
}

#[test]
fn certain_chance_executes_on_every_allowed_check() {
    let mut world = test_world();
    let mut bus = EventBus::with_seed(42);
    bus.add_event(repeating(Record("always"), 10.0, 1.0));
    for expected in 1..=10 {
        bus.tick(&mut world);
        assert_eq!(log(&world).len(), expected);
        bus.tick(&mut world);
        assert_eq!(log(&world).len(), expected);
        advance(&mut world, 10.0);
    }
}

fn seeded_scenario(seed: u64) -> Vec<Vec<&'static str>> {
    let mut world = test_world();
    let mut bus = EventBus::with_seed(seed);
    bus.add_event(repeating(Record("a"), 2.0, 0.25));
    bus.add_event(repeating(Record("b"), 3.0, 0.5));
    (0..100)
        .map(|_| {
            bus.tick(&mut world);
            advance(&mut world, 1.0);
            std::mem::take(&mut world.resource_mut::<TestState>().log)
        })
        .collect()
}

#[test]
fn same_seed_reproduces_the_whole_scenario() {
    let first = seeded_scenario(42);
    assert_eq!(first, seeded_scenario(42));
    assert!(first.iter().flatten().count() > 2);
    assert_ne!(first, seeded_scenario(43));
}

#[test]
fn failed_roll_starts_cooldown_before_next_readiness_check() {
    let mut world = test_world();
    let mut bus = EventBus::with_seed(42);
    // This event panics if readiness is checked twice.
    bus.add_event(repeating(CheckOnce(false), 10.0, 0.0));
    bus.tick(&mut world);
    advance(&mut world, 1.0);
    bus.tick(&mut world);
    assert!(log(&world).is_empty());
}

struct CheckOnce(bool);
impl ScriptedEvent for CheckOnce {
    fn is_ready(&mut self, _: &EventBus, _: &World) -> bool {
        assert!(!self.0, "readiness must not be checked during cooldown");
        self.0 = true;
        true
    }
    fn execute(&self, _: &mut EventBus, _: &mut World) {
        panic!("zero chance event must not execute");
    }
}

#[test]
fn not_ready_does_not_start_cooldown() {
    let mut world = test_world();
    let mut bus = EventBus::with_seed(42);
    bus.add_event(repeating(ReadyAtIteration(1), 10.0, 1.0));
    bus.tick(&mut world);
    advance(&mut world, 1.0);
    world.resource_mut::<TestState>().iteration = 1;
    bus.tick(&mut world);
    assert_eq!(log(&world), &["ready"]);
    advance(&mut world, 10.0);
    bus.tick(&mut world);
    assert_eq!(log(&world), &["ready", "ready"]);
}

#[test]
fn failed_random_roll_waits_until_the_next_interval() {
    // Find a failure then success through the public API, including stream selection.
    let seed = (0..10_000)
        .find(|&seed| {
            let mut world = test_world();
            let mut bus = EventBus::with_seed(seed);
            bus.add_event(repeating(Record("rolled"), 10.0, 0.5));
            bus.tick(&mut world);
            if !log(&world).is_empty() {
                return false;
            }
            advance(&mut world, 10.0);
            bus.tick(&mut world);
            log(&world) == ["rolled"]
        })
        .expect("a seed with a failed then successful roll");
    let mut world = test_world();
    let mut bus = EventBus::with_seed(seed);
    bus.add_event(repeating(Record("rolled"), 10.0, 0.5));
    bus.tick(&mut world);
    assert!(log(&world).is_empty());
    advance(&mut world, 1.0);
    bus.tick(&mut world);
    assert!(log(&world).is_empty());
    advance(&mut world, 9.0);
    bus.tick(&mut world);
    assert_eq!(log(&world), &["rolled"]);
}

#[test]
fn plugin_respects_virtual_time_pause() {
    use bevy::time::{TimePlugin, TimeUpdateStrategy, Virtual};
    let mut app = App::new();
    app.add_plugins((TimePlugin, EventBusPlugin))
        .insert_resource(TestState::default())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)));
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_max_delta(Duration::from_secs(1));
    app.world_mut()
        .resource_mut::<EventBus>()
        .add_event(EventSlot::repeatable(Record("virtual"), 2.0));
    app.update();
    assert_eq!(log(app.world()).len(), 1);
    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(log(app.world()).len(), 1);
    app.world_mut().resource_mut::<Time<Virtual>>().unpause();
    app.update();
    assert_eq!(log(app.world()).len(), 1);
    app.update();
    assert_eq!(log(app.world()).len(), 2);
}

#[test]
fn handles_distinguish_instances_and_pending_cancellation() {
    let mut bus = EventBus::with_seed(42);
    let mut world = test_world();
    let first = bus.add_event(once(Record("first")));
    let second = bus.add_event(once(Record("second")));
    assert_ne!(first, second);
    assert!(bus.is_alive().by_id(first));
    assert!(bus.is_alive().by_type::<Record>());
    bus.delete_event().by_id(first);
    assert!(!bus.is_alive().by_id(first));
    assert!(bus.is_alive().by_id(second));
    bus.tick(&mut world);
    assert_eq!(log(&world), &["second"]);
    assert!(!bus.is_alive().by_id(second));
    assert!(!bus.is_alive().by_type::<Record>());
}

#[test]
fn alive_queries_replay_mixed_pending_operations_in_order() {
    let mut bus = EventBus::with_seed(42);
    let mut world = test_world();
    let old = bus.add_event(repeating(Record("old"), 10.0, 1.0));
    bus.tick(&mut world);
    bus.delete_event().by_type::<Record>();
    let new = bus.add_event(once(Record("new")));
    assert!(!bus.is_alive().by_id(old));
    assert!(bus.is_alive().by_id(new));
    assert!(bus.is_alive().by_type::<Record>());
    bus.delete_event().by_id(new);
    assert!(!bus.is_alive().by_type::<Record>());
    bus.tick(&mut world);
    assert_eq!(log(&world), &["old"]);
}

#[test]
fn repeatable_handle_survives_execution_until_deleted() {
    let mut bus = EventBus::with_seed(42);
    let mut world = test_world();
    let id = bus.add_event(repeating(Record("repeat"), 1.0, 1.0));
    bus.tick(&mut world);
    assert!(bus.is_alive().by_id(id));
    bus.delete_event().by_id(id);
    assert!(!bus.is_alive().by_id(id));
    advance(&mut world, 1.0);
    bus.tick(&mut world);
    assert_eq!(log(&world), &["repeat"]);
}

#[test]
fn death_removes_handle_and_type_membership() {
    let mut bus = EventBus::with_seed(42);
    let mut world = test_world();
    let id = bus.add_event(once(AlreadyDead));
    assert!(bus.is_alive().by_id(id));
    bus.tick(&mut world);
    assert!(!bus.is_alive().by_id(id));
    assert!(!bus.is_alive().by_type::<AlreadyDead>());
}

struct CancelById(crate::EventId);
impl ScriptedEvent for CancelById {
    fn execute(&self, bus: &mut EventBus, _: &mut World) {
        assert!(bus.is_alive().by_id(self.0));
        bus.delete_event().by_id(self.0);
        assert!(!bus.is_alive().by_id(self.0));
    }
}

#[test]
fn cancellation_query_changes_before_physical_removal() {
    let mut bus = EventBus::with_seed(42);
    let mut world = test_world();
    let victim = bus.add_event(once(Victim));
    bus.add_event(once(CancelById(victim)));
    bus.tick(&mut world);
    assert!(!bus.is_alive().by_id(victim));
    world.resource_mut::<TestState>().iteration = 1;
    bus.tick(&mut world);
    assert!(log(&world).is_empty());
}

struct StreamChild;
impl ScriptedEvent for StreamChild {
    fn execute(&self, _: &mut EventBus, world: &mut World) {
        world.resource_mut::<TestState>().log.push("child");
    }
}
struct StreamParent;
impl ScriptedEvent for StreamParent {
    fn execute(&self, bus: &mut EventBus, _: &mut World) {
        let id = bus.add_event(repeating(StreamChild, 1.0, 0.5));
        assert!(bus.is_alive().by_id(id));
        assert!(bus.is_alive().by_type::<StreamChild>());
    }
}

fn stream_trace(noise: bool, child: bool) -> Vec<bool> {
    let mut bus = EventBus::with_seed(42);
    let mut world = test_world();
    if noise {
        bus.add_event(repeating(OtherRecord, 0.0, 0.5));
    }
    if child {
        bus.add_event(once(StreamParent));
    } else {
        bus.add_event(repeating(StreamChild, 1.0, 0.5));
    }
    (0..100)
        .map(|_| {
            bus.tick(&mut world);
            let fired = log(&world).contains(&"child");
            world.resource_mut::<TestState>().log.clear();
            advance(&mut world, 1.0);
            fired
        })
        .collect()
}

#[test]
fn unrelated_root_rng_draws_do_not_shift_event_stream() {
    let trace = stream_trace(false, false);
    assert!(trace.contains(&true) && trace.contains(&false));
    assert_eq!(trace, stream_trace(true, false));
}

#[test]
fn child_rng_stream_is_independent_of_unrelated_root_events() {
    let trace = stream_trace(false, true);
    assert!(!trace[0], "child must wait until the next tick");
    assert!(trace.contains(&true));
    assert_eq!(trace, stream_trace(true, true));
}

#[test]
fn event_name_defaults_to_concrete_type_and_can_be_overridden() {
    let event: Box<dyn ScriptedEvent> = Box::new(Record("named"));
    assert_eq!(event.name(), std::any::type_name::<Record>());
    struct Named;
    impl ScriptedEvent for Named {
        fn execute(&self, _: &mut EventBus, _: &mut World) {}
        fn name(&self) -> &'static str {
            "quest.named.v1"
        }
    }
    let event: Box<dyn ScriptedEvent> = Box::new(Named);
    assert_eq!(event.name(), "quest.named.v1");
}

struct CancelReadyRecord;
impl ScriptedEvent for CancelReadyRecord {
    fn execute(&self, bus: &mut EventBus, _: &mut World) {
        bus.delete_event().by_type::<Record>();
        assert!(!bus.is_alive().by_type::<Record>());
    }
}

#[test]
fn pending_deletion_does_not_skip_a_ready_event_in_the_current_tick() {
    let mut bus = EventBus::with_seed(42);
    let mut world = test_world();
    bus.add_event(once(CancelReadyRecord));
    bus.add_event(repeating(Record("still runs"), 0.0, 1.0));
    bus.tick(&mut world);
    assert_eq!(log(&world), &["still runs"]);
    assert!(!bus.is_alive().by_type::<Record>());
    bus.tick(&mut world);
    assert_eq!(log(&world), &["still runs"]);
}

struct CheckOwnMembership {
    repeatable: bool,
}
impl ScriptedEvent for CheckOwnMembership {
    fn is_ready(&mut self, bus: &EventBus, _: &World) -> bool {
        // The payload has been taken out, but cached type identity remains.
        assert!(bus.is_alive().by_type::<Self>());
        true
    }
    fn execute(&self, bus: &mut EventBus, _: &mut World) {
        assert_eq!(bus.is_alive().by_type::<Self>(), self.repeatable);
    }
}

#[test]
fn membership_during_callbacks_uses_cached_concrete_type() {
    for repeatable in [false, true] {
        let mut bus = EventBus::with_seed(42);
        let mut world = test_world();
        let event = CheckOwnMembership { repeatable };
        let slot = if repeatable {
            repeating(event, 1.0, 1.0)
        } else {
            once(event)
        };
        bus.add_event(slot);
        bus.tick(&mut world);
    }
}

mod combinators {
    use super::*;
    use crate::{After, ScriptedEventExt};

    #[test]
    fn delay_starts_on_first_readiness_check_and_includes_boundary() {
        let mut world = test_world();
        let mut bus = EventBus::with_seed(42);
        bus.add_event(once(Record("delayed").after(5.0)));
        advance(&mut world, 100.0);
        bus.tick(&mut world);
        advance(&mut world, 4.0);
        bus.tick(&mut world);
        assert!(log(&world).is_empty());
        advance(&mut world, 1.0);
        bus.tick(&mut world);
        assert_eq!(log(&world), &["delayed"]);
        bus.tick(&mut world);
        assert_eq!(log(&world), &["delayed"]);
    }

    #[test]
    fn after_short_circuits_then_preserves_mutable_inner_readiness() {
        let mut world = test_world();
        let mut bus = EventBus::with_seed(42);
        bus.add_event(once(
            ReadyAfterChecks {
                checks: 0,
                required: 2,
            }
            .after(5.0),
        ));
        for _ in 0..3 {
            bus.tick(&mut world);
        }
        advance(&mut world, 5.0);
        bus.tick(&mut world);
        assert!(log(&world).is_empty());
        bus.tick(&mut world);
        assert_eq!(log(&world), &["counted"]);
    }

    #[test]
    fn when_short_circuits_then_preserves_inner_readiness() {
        let mut world = test_world();
        let mut bus = EventBus::with_seed(42);
        let threshold = 2;
        bus.add_event(once(
            ReadyAfterChecks {
                checks: 0,
                required: 2,
            }
            .when(move |_, w| w.resource::<TestState>().iteration >= threshold),
        ));
        for _ in 0..3 {
            bus.tick(&mut world);
        }
        world.resource_mut::<TestState>().iteration = 2;
        bus.tick(&mut world);
        assert!(log(&world).is_empty());
        bus.tick(&mut world);
        assert_eq!(log(&world), &["counted"]);
    }

    #[test]
    fn composition_order_controls_when_the_timer_starts() {
        let mut world = test_world();
        let mut bus = EventBus::with_seed(42);
        bus.add_event(once(
            Record("condition first")
                .after(5.0)
                .when(|_, w| w.resource::<TestState>().iteration > 0),
        ));
        bus.add_event(once(
            Record("timer first")
                .when(|_, w| w.resource::<TestState>().iteration > 0)
                .after(5.0),
        ));
        bus.tick(&mut world);
        advance(&mut world, 10.0);
        world.resource_mut::<TestState>().iteration = 1;
        bus.tick(&mut world);
        assert_eq!(log(&world), &["timer first"]);
        advance(&mut world, 5.0);
        bus.tick(&mut world);
        assert_eq!(log(&world), &["timer first", "condition first"]);
    }

    #[test]
    fn false_outer_condition_does_not_reset_or_pause_started_delay() {
        let mut world = test_world();
        let mut bus = EventBus::with_seed(42);
        world.resource_mut::<TestState>().iteration = 1;
        bus.add_event(once(
            Record("ready")
                .after(5.0)
                .when(|_, w| w.resource::<TestState>().iteration > 0),
        ));
        bus.tick(&mut world);
        world.resource_mut::<TestState>().iteration = 0;
        advance(&mut world, 10.0);
        bus.tick(&mut world);
        assert!(log(&world).is_empty());
        world.resource_mut::<TestState>().iteration = 1;
        bus.tick(&mut world);
        assert_eq!(log(&world), &["ready"]);
    }

    #[test]
    fn death_bypasses_both_gates_and_predicate() {
        let mut world = test_world();
        let mut bus = EventBus::with_seed(42);
        let first = bus.add_event(once(
            AlreadyDead
                .after(100.0)
                .when(|_, _| panic!("dead event must not evaluate predicate")),
        ));
        let second = bus.add_event(once(
            AlreadyDead
                .when(|_, _| panic!("dead event must not evaluate predicate"))
                .after(100.0),
        ));
        bus.tick(&mut world);
        assert!(!bus.is_alive().by_id(first));
        assert!(!bus.is_alive().by_id(second));
    }

    #[test]
    fn wrapped_event_can_be_cancelled_by_id_before_delay_expires() {
        let mut world = test_world();
        let mut bus = EventBus::with_seed(42);
        let id = bus.add_event(once(Record("cancelled").after(5.0).when(|_, _| true)));
        bus.tick(&mut world);
        bus.delete_event().by_id(id);
        assert!(!bus.is_alive().by_id(id));
        advance(&mut world, 10.0);
        bus.tick(&mut world);
        assert!(log(&world).is_empty());
    }

    #[test]
    fn wrapper_type_and_name_remain_distinct_from_inner_event() {
        let event = Record("wrapped").after(5.0);
        assert_eq!(event.name(), std::any::type_name::<After<Record>>());
        let mut bus = EventBus::with_seed(42);
        let id = bus.add_event(once(event));
        assert!(bus.is_alive().by_type::<After<Record>>());
        assert!(!bus.is_alive().by_type::<Record>());
        bus.delete_event().by_type::<Record>();
        assert!(bus.is_alive().by_id(id));
        bus.delete_event().by_type::<After<Record>>();
        assert!(!bus.is_alive().by_id(id));
    }

    #[test]
    fn repeating_event_uses_delay_once_then_slot_cooldown() {
        let mut world = test_world();
        let mut bus = EventBus::with_seed(42);
        bus.add_event(repeating(Record("repeat").after(5.0), 2.0, 1.0));
        bus.tick(&mut world);
        advance(&mut world, 5.0);
        bus.tick(&mut world);
        advance(&mut world, 1.0);
        bus.tick(&mut world);
        assert_eq!(log(&world), &["repeat"]);
        advance(&mut world, 1.0);
        bus.tick(&mut world);
        assert_eq!(log(&world), &["repeat", "repeat"]);
    }

    #[test]
    fn wrapped_execute_can_enqueue_successors() {
        let mut world = test_world();
        let mut bus = EventBus::with_seed(42);
        bus.add_event(once(SpawnNext.after(0.0).when(|_, _| true)));
        bus.tick(&mut world);
        assert_eq!(log(&world), &["first"]);
        bus.tick(&mut world);
        assert_eq!(log(&world), &["first", "second"]);
    }

    #[test]
    fn delay_rejects_negative_and_nonfinite_values() {
        for seconds in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(std::panic::catch_unwind(|| Record("invalid").after(seconds)).is_err());
        }
    }

    struct DiesWhileWaiting;
    impl ScriptedEvent for DiesWhileWaiting {
        fn is_dead(&self, _: &EventBus, world: &World) -> bool {
            world.resource::<TestState>().iteration > 0
        }
        fn execute(&self, _: &mut EventBus, _: &mut World) {
            panic!("event should die before its delay elapses");
        }
    }

    #[test]
    fn inner_death_is_checked_while_delay_is_running() {
        let mut world = test_world();
        let mut bus = EventBus::with_seed(42);
        let id = bus.add_event(once(DiesWhileWaiting.after(10.0).when(|_, _| true)));
        bus.tick(&mut world);
        assert!(bus.is_alive().by_id(id));
        world.resource_mut::<TestState>().iteration = 1;
        advance(&mut world, 1.0);
        bus.tick(&mut world);
        assert!(!bus.is_alive().by_id(id));
    }

    #[test]
    fn after_respects_virtual_time_pause() {
        use bevy::time::{TimePlugin, TimeUpdateStrategy, Virtual};
        let mut app = App::new();
        app.add_plugins((TimePlugin, EventBusPlugin))
            .insert_resource(TestState::default())
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)));
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(Duration::from_secs(1));
        app.world_mut()
            .resource_mut::<EventBus>()
            .add_event(once(Record("delayed").after(2.0)));
        app.update();
        app.update();
        assert!(log(app.world()).is_empty());
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        for _ in 0..5 {
            app.update();
        }
        assert!(log(app.world()).is_empty());
        app.world_mut().resource_mut::<Time<Virtual>>().unpause();
        app.update();
        assert_eq!(log(app.world()), &["delayed"]);
    }
}
