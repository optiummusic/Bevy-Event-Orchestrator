//! Event slot type
use rand_chacha::ChaCha8Rng;

use crate::{EventId, ScriptedEvent};

/// Cooldown and probability for repeated attempts. The first attempt is immediately
/// eligible; subsequent attempts wait from the last roll, successful or not.
pub struct Repeatable {
    /// Minimum game-time seconds between eligible probability checks.
    /// Use a finite nonnegative value; zero permits one attempt each tick.
    pub cooldown_secs: f64,
    /// Probability per eligible check, from 0.0 (never) to 1.0 (always).
    pub chance: f32,
    last_checked: Option<f64>,
}

impl Repeatable {
    /// Creates a repeat policy with certain execution when ready.
    pub fn new(cooldown_secs: f64) -> Self {
        Self {
            cooldown_secs,
            chance: 1.0,
            last_checked: None,
        }
    }
    pub(crate) fn chance(&self) -> f32 {
        self.chance
    }
    /// Creates a repeat policy with the given probability.
    /// Values are stored without validation or clamping; callers should supply
    /// a finite nonnegative cooldown and a probability in `0.0..=1.0`.
    pub fn with_chance(cooldown_secs: f64, chance: f32) -> Self {
        Self {
            cooldown_secs,
            chance,
            last_checked: None,
        }
    }

    pub(crate) fn cooldown_elapsed(&self, now: f64) -> bool {
        self.last_checked
            .map(|last_checked| now - last_checked >= self.cooldown_secs)
            .unwrap_or(true)
    }

    pub(crate) fn roll(&mut self, now: f64, sample: f32) -> bool {
        if !self.cooldown_elapsed(now) {
            return false;
        }
        self.last_checked = Some(now);
        sample < self.chance
    }
}

/// An event and its optional repeat policy. The event is temporarily taken out
/// during callbacks so they can access the bus without a borrow conflict.
///
/// Construct slots with [`Self::once`], [`Self::repeatable`], or
/// [`Self::repeatable_chance`]. Empty slots are not part of the public API.
///
/// ```compile_fail
/// use bevy_event_orchestrator::EventSlot;
/// let empty = EventSlot::default();
/// ```
pub struct EventSlot {
    pub(crate) event: Option<Box<dyn ScriptedEvent>>,
    pub(crate) repeat: Option<Repeatable>,
    pub(crate) id: EventId,
    pub(crate) key: u64,
    pub(crate) explicit_key: Option<u64>,
    pub(crate) rng: Option<ChaCha8Rng>,
    pub(crate) type_id: Option<std::any::TypeId>,
}

impl EventSlot {
    fn build(event: Box<dyn ScriptedEvent>, repeat: Option<Repeatable>) -> Self {
        // Compute before moving the box. The deref gives the event's TypeId, not Box's.
        let type_id = {
            let any: &dyn std::any::Any = &*event;
            any.type_id()
        };
        Self {
            event: Some(event),
            repeat,
            type_id: Some(type_id),
            id: EventId::default(),
            key: 0,
            explicit_key: None,
            rng: None,
        }
    }

    pub(crate) fn event_type_id(&self) -> Option<std::any::TypeId> {
        self.type_id
    }

    /// Repeats when ready, with at most one execution per cooldown interval.
    pub fn repeatable(event: Box<dyn ScriptedEvent>, cooldown_secs: f64) -> Self {
        Self::build(event, Some(Repeatable::new(cooldown_secs)))
    }

    /// Uses a per-event random stream for probability checks when ready.
    /// Every eligible check restarts the cooldown, including failed checks.
    pub fn repeatable_chance(
        event: Box<dyn ScriptedEvent>,
        cooldown_secs: f64,
        chance: f32,
    ) -> Self {
        Self::build(event, Some(Repeatable::with_chance(cooldown_secs, chance)))
    }

    /// Executes once when ready, then removes the event.
    pub fn once(event: Box<dyn ScriptedEvent>) -> Self {
        Self::build(event, None)
    }
}
