# Bevy Event Orchestrator

Build a game scenario as a chain of events. Each event waits for its own world
condition, performs an action, and registers what happens next with
`bus.add_event()` inside `execute()`.

A single initial event can grow into an ambush, parallel objectives, a player
choice, a reward, and an epilogue. Repeatable events add cooldowns and seeded
probability checks to the same chain.

## Play the caravan encounter

The complete runnable scenario is in [`examples/examples.rs`](examples/examples.rs).
It runs without a window and prints a timestamped story:

```sh
cargo run --example examples -- rescue
cargo run --example examples -- leave
cargo run --example examples -- fail
```

Only `EnterCanyon` is registered at startup. Everything below it is registered
by another event's `execute()`:

```text
EnterCanyon — wait for the player to enter
  └─ add Ambush — wait two game-time seconds
       ├─ add Reinforcements — 50% chance every 2s, at most two arrivals
       ├─ add BattleWon — wait for zero enemies before the deadline
       │    └─ add ChooseRoute
       │         ├─ rescue: add FreePrisoner + TakeSupplies + Reward
       │         │            └─ Reward waits for BOTH objectives → 150 gold
       │         └─ leave:  add TakeSupplies + Reward
       │                      └─ Reward waits for supplies → 50 gold
       │                         └─ add Epilogue
       └─ add CaravanLost — deadline reached while battle is active
            └─ add Epilogue — failure, no reward
```

Both successful routes reach the epilogue through `Reward`. Victory and failure
are mutually exclusive: victory requires time strictly before the deadline,
while failure requires the deadline to have elapsed. Victory cancels the timeout and reinforcements by their individual `EventId`s.
Failure cancels its reinforcements. `is_dead()` guards obsolete events during
the current tick, before deferred deletion is applied. The example also queries
pending membership with `is_alive().by_id(id)` and checks cleanup by type.

The rescue route demonstrates a **join**: freeing the prisoner and recovering
supplies run independently, and the reward waits for both world conditions.
It does not depend on the bus visiting those events in a particular order.

A separate Bevy system simulates movement and combat. It never registers an
event or advances a quest stage. In a real game, replace it with your movement,
combat, and player-input systems. The command-line argument selects the player's
choice ahead of time for this demonstration.

Each run uses seed 42 and advances virtual time manually, one second per update.
Forty updates finish immediately without sleeping; assertions check the selected
outcome, reward, and exactly one epilogue. Exact reinforcement rolls can change
between RNG versions, but the scenario remains bounded and all routes finish.

## Compatibility

| Crate | Bevy | Rust |
| --- | --- | --- |
| 0.1.x | 0.20 | 1.97.1 or newer |

The crate disables Bevy's default features; it does not require a renderer.
The declared Rust minimum follows Bevy 0.20's requirement.

## Installation

After the initial crates.io release:

```toml
[dependencies]
bevy = "0.20"
bevy-event-orchestrator = "0.1"
```

Before publication, use a local path to this crate instead of the version.

## The core pattern: execute, then add the next event

This small executable example isolates the same transition used throughout the
caravan scenario. The reward is created by the quest event, not pre-registered
by the application:

```rust
use bevy::prelude::*;
use bevy_event_orchestrator::{EventBus, EventBusPlugin, EventSlot, ScriptedEvent};

#[derive(Resource, Default)]
struct Gold(u32);

struct CompleteQuest;
impl ScriptedEvent for CompleteQuest {
    fn execute(&self, bus: &mut EventBus, _world: &mut World) {
        // Choose the next event here, using the current world state if needed.
        bus.add_event(EventSlot::once(GrantReward { amount: 150 }));
    }
}

struct GrantReward { amount: u32 }
impl ScriptedEvent for GrantReward {
    fn execute(&self, _bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<Gold>().0 += self.amount;
    }
}

let mut app = App::new();
app.add_plugins(MinimalPlugins)
    .insert_resource(EventBus::with_seed(42))
    .add_plugins(EventBusPlugin)
    .init_resource::<Gold>();

app.world_mut().resource_mut::<EventBus>()
    .add_event(EventSlot::once(CompleteQuest));

app.update(); // CompleteQuest runs and queues GrantReward.
assert_eq!(app.world().resource::<Gold>().0, 0);
app.update(); // GrantReward becomes eligible and runs.
assert_eq!(app.world().resource::<Gold>().0, 150);
app.update(); // Both one-shot events are gone.
assert_eq!(app.world().resource::<Gold>().0, 150);
```

Override `is_ready()` to wait for a world condition or a deadline, as the caravan
example does. A queued successor is first checked on the next tick; it can then
wait as long as necessary. Events added during execution do not recursively
execute in the same tick.

## Compose conditions and delays

Import `ScriptedEventExt` to wrap any event with `.after(seconds)` and
`.when(predicate)`. Both preserve the original event's `is_ready()`, `is_dead()`,
and `execute()`; successors added inside `execute()` still form normal chains.

```rust
use std::time::Duration;
use bevy::prelude::*;
use bevy_event_orchestrator::{EventBus, EventSlot, ScriptedEvent, ScriptedEventExt};

#[derive(Resource, Default)]
struct Night(bool);
#[derive(Resource, Default)]
struct Encounters(Vec<&'static str>);

struct SpawnMonster(&'static str);
impl ScriptedEvent for SpawnMonster {
    fn execute(&self, _: &mut EventBus, world: &mut World) {
        world.resource_mut::<Encounters>().0.push(self.0);
    }
}

let mut world = World::new();
world.insert_resource(Time::<()>::default());
world.insert_resource(Night::default());
world.insert_resource(Encounters::default());
let mut bus = EventBus::with_seed(42);

bus.add_event(EventSlot::once(
    SpawnMonster("wait after nightfall").after(5.0)
        .when(|_, world| world.resource::<Night>().0),
));
bus.add_event(EventSlot::once(
    SpawnMonster("wait from first check")
        .when(|_, world| world.resource::<Night>().0).after(5.0),
));

bus.tick(&mut world); // Time 0: only the outer After starts its timer.
world.resource_mut::<Time>().advance_by(Duration::from_secs(10));
world.resource_mut::<Night>().0 = true;
bus.tick(&mut world); // Nightfall: the inner After starts its timer now.
assert_eq!(world.resource::<Encounters>().0, ["wait from first check"]);
world.resource_mut::<Time>().advance_by(Duration::from_secs(5));
bus.tick(&mut world);
assert_eq!(world.resource::<Encounters>().0,
    ["wait from first check", "wait after nightfall"]);
```

The last wrapper is checked first. `event.after(5.0).when(condition)` delays
starting its timer until the condition first allows a readiness check.
`event.when(condition).after(5.0)` starts its timer on the first readiness check
and evaluates the condition once the delay expires. Construction, registration,
and waiting behind another gate do not start an inner timer.

A started timer keeps its deadline even if an outer predicate becomes false.
The predicate must be true again for execution, but it does not pause or reset
the timer. `After` uses Bevy game time, so a paused clock does stop elapsed time.
For repeatable slots, the delay applies only initially; later attempts follow
the slot's cooldown. Death checks bypass all gates and can cancel the event
before its delay expires. Delays must be finite and nonnegative; invalid values
panic at construction. Zero delay checks inner readiness immediately.

Wrappers retain their own concrete identity and default type name: use
`delete_event().by_type::<After<SpawnMonster>>()` for an `After<SpawnMonster>`,
not `by_type::<SpawnMonster>()`. For `When` with an anonymous closure, keep the
`EventId` returned by `add_event()` and cancel by ID. Wrapping an event also
changes its root RNG key; the inner event's custom `name()` is not forwarded.
Rust type names are not guaranteed unique identifiers, particularly for closure
types; they must not be treated as a collision-free RNG namespace.

## Repeated events

Pass events by value to all three constructors; `EventSlot` handles boxing internally.

Wrap your event with `EventSlot::repeatable(event, 10.0)` to attempt
execution every ten game-time seconds, or
`EventSlot::repeatable_chance(event, 10.0, 0.25)` for a 25% chance per
eligible attempt. The first attempt is eligible immediately.

Every tick checks events in this order:

1. `is_dead()` removes the event permanently when true, even during cooldown.
2. An active cooldown skips the remaining checks.
3. `is_ready()` must return true before a probability check is made.
4. Repeatable events use their own ChaCha8 RNG stream. Every eligible attempt
   starts a cooldown, including a failed attempt. Chance 1 skips the RNG draw.
5. Successful events execute; one-shot events are removed and repeatable events
   remain registered.

A false readiness result consumes neither randomness nor cooldown. There is at
most one attempt per event per tick, with no catch-up for missed intervals.
Use finite, nonnegative cooldowns and probabilities in `0.0..=1.0`; inputs are
not validated or clamped. A zero cooldown permits one attempt per tick.

## Time and determinism

The bus reads `Time::elapsed_secs_f64()`. In the plugin's `Update` schedule this
is Bevy virtual game time, so positive cooldowns follow pause and time scaling.
Pausing time does not disable the bus: immediately eligible events can still
execute. Add your own readiness condition when the whole scenario must pause.
Manual `World` users must insert and advance a `Time` resource themselves.

`EventBus::new()` chooses a random bus seed. `EventBus::with_seed(seed)` derives
an individual `ChaCha8Rng` stream for each repeatable event:

- Root keys depend on `ScriptedEvent::name()` and the registration number for
  that concrete event type. Adding a different root type does not shift the
  existing type's stream; adding an earlier root of the same type does.
- Child keys depend on the parent key and the child's position in that
  `execute()` call. Unrelated chains do not consume each other's random draws.
- `name()` defaults to the concrete Rust type name. Override it with a distinct,
  stable string to decouple root keys from a type/module rename.

The same seed, keys, and eligible checks reproduce draws with the same algorithm
and sampling behavior. World mutations and timing can still change readiness,
so separate RNG streams do not make the entire simulation order-independent.

**Current limitation:** each execution resets the parent's child counter.
Children created at the same position on successive executions of a repeatable
parent reuse a stream from its beginning. Different types overriding `name()`
with the same value can also derive matching root keys. There is currently no
public explicit-key setter. Do not assume every registration has a unique stream
or that arbitrary refactors preserve replays.

## Handles, cancellation, and membership

`add_event(slot)` returns an `EventId` immediately. Store it when you need to
cancel one registration without affecting other events of the same type:

```rust
use bevy::prelude::*;
use bevy_event_orchestrator::{EventBus, EventSlot, ScriptedEvent};

struct Patrol;
impl ScriptedEvent for Patrol {
    fn execute(&self, _: &mut EventBus, _: &mut World) {}
}

let mut bus = EventBus::with_seed(42);
let first = bus.add_event(EventSlot::repeatable(Patrol, 5.0));
let second = bus.add_event(EventSlot::repeatable(Patrol, 5.0));
assert!(bus.is_alive().by_id(first)); // Includes pending additions.

bus.delete_event().by_id(first);
assert!(!bus.is_alive().by_id(first)); // Includes pending deletions too.
assert!(bus.is_alive().by_id(second));
assert!(bus.is_alive().by_type::<Patrol>());

bus.delete_event().by_type::<Patrol>(); // Cancel every Patrol registration.
assert!(!bus.is_alive().by_type::<Patrol>());
```

IDs belong to their originating bus; another bus can issue an equal ID. They are
registration handles, not persistent save-file identifiers. `EventSlot` has no
`Default`: construct it with `once`, `repeatable`, or `repeatable_chance`.
Implementing `ScriptedEvent` requires only `execute`; `as_any` is not needed.

`is_alive()` replays pending operations in call order. It reports projected
membership, not whether `is_ready()` or `is_dead()` will allow execution.
During `execute()`, a one-shot event is already removed; a repeatable event is
still registered unless a pending deletion hides it from the query.

## Queue and scheduling

Additions and deletions are applied in call order at the beginning of the next
bus tick. A deletion queued during a callback immediately affects `is_alive()`,
but does **not** prevent an event later in the current tick from executing.
Use an `is_dead()` condition tied to world state when immediate cancellation is
required. World mutations are immediately visible.

Use `.before(EventBusSet)` or `.after(EventBusSet)` on your `Update` systems to
control their position relative to the bus. Import `EventBusSet` from this crate
and Bevy's prelude for the scheduling methods.

## Migrating from the earlier API

- Pass events directly: `EventSlot::once(MyEvent)` instead of wrapping them in `Box::new`.

- Remove `as_any()` implementations and unused `Any` imports.
- Replace `delete_event::<T>()` with `delete_event().by_type::<T>()`.
- Retain the ID returned by `add_event()` for `delete_event().by_id(id)`.
- Use `new()` or `with_seed()`; the former `with_rng(StdRng)` is gone.
- Seeded outcomes differ from the previous shared-`StdRng` implementation.

## Scope and limitations

- The bus scans all registered events each tick and executes callbacks serially
  with exclusive world access. Keep checks and callbacks short.
- Removal may reorder remaining events. Registration order is not a guaranteed
  execution order; express dependencies through world conditions or later ticks.
- During plugin callbacks, the bus is temporarily removed from `World`. Use the
  supplied `bus` parameter instead of fetching `EventBus` from the world.
- Do not recursively call `tick()` from callbacks. Callback panics propagate;
  recovery after a callback panic is not supported.
- There are no priorities, persistence, or network replay guarantees
  in this version. The API may evolve before 1.0.

## Development

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo run --locked --example examples -- rescue
cargo run --locked --example examples -- leave
cargo run --locked --example examples -- fail
```

See `RELEASING.md` for packaging and publication steps.

## License

Licensed under either the MIT license (`LICENSE-MIT`) or the Apache License,
Version 2.0 (`LICENSE-APACHE`), at your option.
