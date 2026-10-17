# Bevy Event Orchestrator

Condition-driven scripted events for Bevy: one-shot actions, repeatable actions,
cooldowns, and probability checks with a seedable random generator.

Useful for encounter directors, quest conditions, ambient events, and simulation
rules. Each event owns its readiness logic and can queue further events.

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

## Quick start

```rust
use std::any::Any;
use bevy::prelude::*;
use bevy_event_orchestrator::{EventBus, EventBusPlugin, EventSlot, ScriptedEvent};

#[derive(Resource, Default)]
struct Score(u32);

struct AwardPoint;

impl ScriptedEvent for AwardPoint {
    fn execute(&self, _bus: &mut EventBus, world: &mut World) {
        world.resource_mut::<Score>().0 += 1;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

let mut app = App::new();
app.add_plugins(MinimalPlugins)
    .insert_resource(EventBus::with_seed(42))
    .add_plugins(EventBusPlugin)
    .init_resource::<Score>();

app.world_mut()
    .resource_mut::<EventBus>()
    .add_event(EventSlot::once(Box::new(AwardPoint)));

app.update();
assert_eq!(app.world().resource::<Score>().0, 1);
```

Run the standalone example with `cargo run --example basic`.
In a game, use `DefaultPlugins` and `app.run()` as usual.

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
cargo run --locked --example basic
```

See `RELEASING.md` for packaging and publication steps.

## License

Licensed under either the MIT license (`LICENSE-MIT`) or the Apache License,
Version 2.0 (`LICENSE-APACHE`), at your option.
