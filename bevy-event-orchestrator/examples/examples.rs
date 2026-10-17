//! A branching encounter built by events registering their successors.
//! Run: cargo run --example examples -- rescue|leave|fail
use std::{any::Any, time::Duration};

use bevy::{prelude::*, time::TimeUpdateStrategy};
use bevy_event_orchestrator::{EventBus, EventBusPlugin, EventBusSet, EventSlot, ScriptedEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Rescue,
    Leave,
    Fail,
}

#[derive(Resource)]
struct Encounter {
    choice: Choice,
    entered: bool,
    active: bool,
    enemies: u32,
    reinforcements: u32,
    prisoner_freed: bool,
    supplies_taken: bool,
    gold: u32,
    finished: bool,
    failed: bool,
    // Records actual execution times to verify deferred transitions.
    journal: Vec<(&'static str, f64)>,
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn record(world: &mut World, message: &'static str) {
    let time = now(world);
    world
        .resource_mut::<Encounter>()
        .journal
        .push((message, time));
    println!("[{time:>4.0}s] {message}");
}

// Each event implements as_any explicitly: no custom framework or helper macro.
struct EnterCanyon;
impl ScriptedEvent for EnterCanyon {
    fn is_ready(&mut self, _: &EventBus, world: &World) -> bool {
        world.resource::<Encounter>().entered
    }

    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        record(world, "Entered the canyon. The scout spots an ambush.");
        // Carry a deadline in the successor itself; no global stage switch.
        bus.add_event(EventSlot::once(Box::new(Ambush {
            at: now(world) + 2.0,
        })));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct Ambush {
    at: f64,
}
impl ScriptedEvent for Ambush {
    fn is_ready(&mut self, _: &EventBus, world: &World) -> bool {
        now(world) >= self.at
    }

    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        let deadline = now(world) + 12.0;
        let mut encounter = world.resource_mut::<Encounter>();
        encounter.active = true;
        encounter.enemies = 4;
        record(
            world,
            "Ambush! Defeat the bandits before the caravan is lost.",
        );

        // Fan out: a repeating pressure source and two competing outcomes.
        bus.add_event(EventSlot::repeatable_chance(
            Box::new(Reinforcements),
            2.0,
            0.5,
        ));
        bus.add_event(EventSlot::once(Box::new(BattleWon { deadline })));
        bus.add_event(EventSlot::once(Box::new(CaravanLost { deadline })));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct Reinforcements;
impl ScriptedEvent for Reinforcements {
    fn is_dead(&self, _: &EventBus, world: &World) -> bool {
        let encounter = world.resource::<Encounter>();
        !encounter.active || encounter.reinforcements >= 2
    }

    fn is_ready(&mut self, _: &EventBus, world: &World) -> bool {
        world.resource::<Encounter>().enemies > 0
    }

    fn execute(&self, _: &mut EventBus, world: &mut World) {
        let mut encounter = world.resource_mut::<Encounter>();
        encounter.enemies += 1;
        encounter.reinforcements += 1;
        record(world, "A bandit reinforcement joins the fight.");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct BattleWon {
    deadline: f64,
}
impl ScriptedEvent for BattleWon {
    fn is_dead(&self, _: &EventBus, world: &World) -> bool {
        !world.resource::<Encounter>().active
    }

    fn is_ready(&mut self, _: &EventBus, world: &World) -> bool {
        // At the deadline, failure wins regardless of the bus's iteration order.
        world.resource::<Encounter>().enemies == 0 && now(world) < self.deadline
    }

    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<Encounter>().active = false;
        record(world, "Battle won. A prisoner calls from the bandit camp.");
        bus.add_event(EventSlot::once(Box::new(ChooseRoute)));
        // Reinforcements and the losing outcome retire through is_dead().
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct CaravanLost {
    deadline: f64,
}
impl ScriptedEvent for CaravanLost {
    fn is_dead(&self, _: &EventBus, world: &World) -> bool {
        !world.resource::<Encounter>().active
    }

    fn is_ready(&mut self, _: &EventBus, world: &World) -> bool {
        now(world) >= self.deadline
    }

    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        let mut encounter = world.resource_mut::<Encounter>();
        encounter.active = false;
        encounter.failed = true;
        record(world, "Too late. The caravan is lost; retreat!");
        bus.add_event(EventSlot::once(Box::new(Epilogue)));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct ChooseRoute;
impl ScriptedEvent for ChooseRoute {
    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        match world.resource::<Encounter>().choice {
            Choice::Rescue => {
                record(
                    world,
                    "You choose to rescue the prisoner and recover supplies.",
                );
                // Fan out into independent tasks, then join their world conditions.
                bus.add_event(EventSlot::once(Box::new(FreePrisoner {
                    at: now(world) + 3.0,
                })));
                bus.add_event(EventSlot::once(Box::new(TakeSupplies {
                    at: now(world) + 1.0,
                })));
                bus.add_event(EventSlot::once(Box::new(Reward { rescue: true })));
            }
            Choice::Leave => {
                record(
                    world,
                    "You choose the supplies and leave the prisoner behind.",
                );
                bus.add_event(EventSlot::once(Box::new(TakeSupplies {
                    at: now(world) + 1.0,
                })));
                bus.add_event(EventSlot::once(Box::new(Reward { rescue: false })));
            }
            Choice::Fail => unreachable!("the demonstration's failure route never wins combat"),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct FreePrisoner {
    at: f64,
}
impl ScriptedEvent for FreePrisoner {
    fn is_ready(&mut self, _: &EventBus, world: &World) -> bool {
        now(world) >= self.at
    }

    fn execute(&self, _: &mut EventBus, world: &mut World) {
        world.resource_mut::<Encounter>().prisoner_freed = true;
        record(world, "The prisoner is free and reveals a hidden treasure.");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct TakeSupplies {
    at: f64,
}
impl ScriptedEvent for TakeSupplies {
    fn is_ready(&mut self, _: &EventBus, world: &World) -> bool {
        now(world) >= self.at
    }

    fn execute(&self, _: &mut EventBus, world: &mut World) {
        world.resource_mut::<Encounter>().supplies_taken = true;
        record(world, "The caravan supplies are recovered.");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct Reward {
    rescue: bool,
}
impl ScriptedEvent for Reward {
    fn is_ready(&mut self, _: &EventBus, world: &World) -> bool {
        let encounter = world.resource::<Encounter>();
        encounter.supplies_taken && (!self.rescue || encounter.prisoner_freed)
    }

    fn execute(&self, bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<Encounter>().gold += if self.rescue { 150 } else { 50 };
        record(world, "All chosen objectives complete. Reward granted.");
        bus.add_event(EventSlot::once(Box::new(Epilogue)));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct Epilogue;
impl ScriptedEvent for Epilogue {
    fn execute(&self, _: &mut EventBus, world: &mut World) {
        world.resource_mut::<Encounter>().finished = true;
        record(world, "The caravan's story ends.");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Stand-in for movement and combat systems. This system never queues events
// and knows nothing about the quest chain or its transitions.
fn simulate_player(time: Res<Time>, mut encounter: ResMut<Encounter>) {
    if time.elapsed_secs_f64() >= 2.0 {
        encounter.entered = true;
    }
    if encounter.active && encounter.choice != Choice::Fail {
        encounter.enemies = encounter.enemies.saturating_sub(1);
    }
}

fn run(choice: Choice) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
        .insert_resource(EventBus::with_seed(42))
        .insert_resource(Encounter {
            choice,
            entered: false,
            active: false,
            enemies: 0,
            reinforcements: 0,
            prisoner_freed: false,
            supplies_taken: false,
            gold: 0,
            finished: false,
            failed: false,
            journal: Vec::new(),
        })
        .add_plugins(EventBusPlugin)
        .add_systems(Update, simulate_player.before(EventBusSet));
    // Bevy normally clamps large frame deltas. Allow one simulated second per update.
    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_max_delta(Duration::from_secs(1));

    // The ONLY initial registration. Every other event comes from execute().
    app.world_mut()
        .resource_mut::<EventBus>()
        .add_event(EventSlot::once(Box::new(EnterCanyon)));

    // No sleeping, rendering, or wall-clock dependence; terminate even on a broken chain.
    for _ in 0..40 {
        app.update();
    }
    let encounter = app.world().resource::<Encounter>();
    assert!(
        encounter.finished,
        "the event chain must reach its epilogue"
    );
    assert_eq!(
        encounter.gold,
        match choice {
            Choice::Rescue => 150,
            Choice::Leave => 50,
            Choice::Fail => 0,
        }
    );
    assert_eq!(encounter.prisoner_freed, choice == Choice::Rescue);
    assert_eq!(encounter.supplies_taken, choice != Choice::Fail);
    assert_eq!(encounter.failed, choice == Choice::Fail);
    assert!(!encounter.active);
    let endings: Vec<_> = encounter
        .journal
        .iter()
        .filter(|(message, _)| *message == "The caravan's story ends.")
        .collect();
    assert_eq!(endings.len(), 1, "the story must finish exactly once");
    let previous = encounter.journal[encounter.journal.len() - 2].1;
    assert!(
        endings[0].1 > previous,
        "a queued epilogue runs on a later tick"
    );
    println!(
        "Outcome: {choice:?}; gold: {}; prisoner rescued: {}",
        encounter.gold, encounter.prisoner_freed
    );
}

fn main() {
    let choice = match std::env::args().nth(1).as_deref() {
        None | Some("rescue") => Choice::Rescue,
        Some("leave") => Choice::Leave,
        Some("fail") => Choice::Fail,
        _ => {
            eprintln!("Usage: cargo run --example examples -- rescue|leave|fail");
            std::process::exit(2);
        }
    };
    run(choice);
}
