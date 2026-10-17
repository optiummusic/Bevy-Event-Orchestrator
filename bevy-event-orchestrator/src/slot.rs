//! Event slot type
use crate::ScriptedEvent;

/// One slot in the event queue. `event` is an Option so repeatable events
/// can be temporarily taken out (via `.take()`) for execution without a
/// borrow conflict on `GameManager`.
pub struct Repeatable {
    pub cooldown_secs: f64,
    pub chance: f32,
    last_checked: Option<f64>,
}

impl Repeatable {
    pub fn new(cooldown_secs: f64) -> Self {
        Self { cooldown_secs, chance: 1.0, last_checked: None }
    }

    pub fn with_chance(cooldown_secs: f64, chance: f32) -> Self {
        Self { cooldown_secs, chance, last_checked: None }
    }

    fn interval_elapsed(&self, now: f64) -> bool {
        self.last_checked.map(|lf| now - lf >= self.cooldown_secs).unwrap_or(true)
    }

    fn roll(&mut self, now: f64, sample: f32) -> bool {
        if !self.interval_elapsed(now) { return false; }
        self.last_checked = Some(now);
        sample < self.chance
    }
}

pub struct EventSlot {
    pub(crate) event: Option<Box<dyn ScriptedEvent>>,
    pub(crate) repeat: Option<Repeatable>,
}

impl EventSlot {
    pub fn once(event: Box<dyn ScriptedEvent>) -> Self {
        Self { event: Some(event), repeat: None}
    }
}