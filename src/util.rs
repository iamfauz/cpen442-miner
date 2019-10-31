use std::time::{Duration, Instant};

pub struct Timer {
    start : Instant,
    period : Duration
}

impl Timer {
    pub fn new(period : Duration) -> Self {
        Timer {
            start : Instant::now(),
            period
        }
    }

    pub fn check_and_reset(&mut self) -> bool {
        if self.start.elapsed() > self.period {
            let rem = self.start.elapsed() - self.period;
            self.start = Instant::now() - rem;
            true
        } else {
            false
        }
    }
}

