//! Event slot type
use crate::ScriptedEvent;

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
pub struct EventSlot {
    pub(crate) event: Option<Box<dyn ScriptedEvent>>,
    pub(crate) repeat: Option<Repeatable>,
}

impl EventSlot {
    /// Repeats whenever ready and the cooldown has elapsed.
    pub fn repeatable(event: Box<dyn ScriptedEvent>, cooldown_secs: f64) -> Self {
        Self {
            event: Some(event),
            repeat: Some(Repeatable::new(cooldown_secs)),
        }
    }

    /// Rolls `chance` after each cooldown when ready. Failed rolls also restart
    /// the cooldown. Use a probability from 0.0 (never) to 1.0 (always).
    pub fn repeatable_chance(
        event: Box<dyn ScriptedEvent>,
        cooldown_secs: f64,
        chance: f32,
    ) -> Self {
        Self {
            event: Some(event),
            repeat: Some(Repeatable::with_chance(cooldown_secs, chance)),
        }
    }

    /// Executes once when ready, then removes the event.
    pub fn once(event: Box<dyn ScriptedEvent>) -> Self {
        Self {
            event: Some(event),
            repeat: None,
        }
    }
}
