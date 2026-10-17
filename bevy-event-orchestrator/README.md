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
while failure requires the deadline to have elapsed. The losing watcher and
reinforcements retire through `is_dead()` when the battle ends.

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
use std::any::Any;
use bevy::prelude::*;
use bevy_event_orchestrator::{EventBus, EventBusPlugin, EventSlot, ScriptedEvent};

#[derive(Resource, Default)]
struct Gold(u32);

struct CompleteQuest;
impl ScriptedEvent for CompleteQuest {
    fn execute(&self, bus: &mut EventBus, _world: &mut World) {
        // Choose the next event here, using the current world state if needed.
        bus.add_event(EventSlot::once(Box::new(GrantReward { amount: 150 })));
    }

    fn as_any(&self) -> &dyn Any { self }
}

struct GrantReward { amount: u32 }
impl ScriptedEvent for GrantReward {
    fn execute(&self, _bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<Gold>().0 += self.amount;
    }

    fn as_any(&self) -> &dyn Any { self }
}

let mut app = App::new();
app.add_plugins(MinimalPlugins)
    .insert_resource(EventBus::with_seed(42))
    .add_plugins(EventBusPlugin)
    .init_resource::<Gold>();

app.world_mut().resource_mut::<EventBus>()
    .add_event(EventSlot::once(Box::new(CompleteQuest)));

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

## Repeated events

Wrap your event with `EventSlot::repeatable(Box::new(event), 10.0)` to attempt
execution every ten game-time seconds, or
`EventSlot::repeatable_chance(Box::new(event), 10.0, 0.25)` for a 25% chance per
eligible attempt. The first attempt is eligible immediately.

Every tick checks events in this order:

1. `is_dead()` removes the event permanently when true, even during cooldown.
2. An active cooldown skips the remaining checks.
3. `is_ready()` must return true before a probability check is made.
4. Repeatable events draw from the bus's shared RNG. Every draw starts a new
   cooldown, including a failed draw.
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

`EventBus::new()` uses random initialization. `EventBus::with_seed(seed)` gives
repeatable draws for identical event registration, time, world state, and calls
within the same RNG algorithm version. `StdRng` does not promise the same stream
across library versions. Adding or removing events can change later draws.

## Queue and scheduling

`add_event()` and `delete_event::<T>()` enqueue operations. They are applied in
call order at the beginning of the next bus tick. Deletion removes every event
of that concrete type. Operations requested during callbacks apply on the next
tick; changes to world resources and entities are immediately visible.

Use `.before(EventBusSet)` or `.after(EventBusSet)` on your `Update` systems to
control their position relative to the bus. Import `EventBusSet` from this crate
and Bevy's prelude for the scheduling methods.

## Scope and limitations

- The bus scans all registered events each tick and executes callbacks serially
  with exclusive world access. Keep checks and callbacks short.
- Removal may reorder remaining events. Registration order is not a guaranteed
  execution order; express dependencies through world conditions or later ticks.
- During plugin callbacks, the bus is temporarily removed from `World`. Use the
  supplied `bus` parameter instead of fetching `EventBus` from the world.
- Do not recursively call `tick()` from callbacks. Callback panics propagate;
  recovery after a callback panic is not supported.
- There are no event handles, priorities, persistence, or network replay guarantees
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
