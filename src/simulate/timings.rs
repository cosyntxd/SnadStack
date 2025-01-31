use std::{
    sync::{Barrier, Mutex},
    time::Instant,
};

/// Useful for debugging deadlocks. Just a thin wrapper over `Barrier``
pub struct BarrierWrapper {
    barrier: Barrier,
    count: Mutex<usize>,
    max: usize,
    name: &'static str,
}
impl BarrierWrapper {
    pub fn new(amt: usize, name: &'static str) -> Self {
        Self {
            barrier: Barrier::new(amt),
            count: Mutex::new(0),
            max: amt,
            name,
        }
    }
    #[cfg(not(debug_assertions))]
    pub fn wait(&mut self) {
        self.barrier.wait();
    }
    #[cfg(debug_assertions)]
    pub fn wait(&mut self) {
        let info = format!("[Thread {}] [Barrier {}]", 0, self.name);
        {
            *self.count.lock().unwrap() += 1;
        }

        let waiting = self.barrier.wait();

        if waiting.is_leader() {
            *self.count.lock().unwrap() = 0;
        }

        println!("{info} ({}/{}) is finished", self.max, self.max);
    }
}

#[derive(Clone, Copy)]
pub enum IterationSteps {
    SleepSecs(f64),
    Iterations(usize),
    Debug(usize),
}

pub struct IterationCount {
    last_tick: Instant,
    target_rate: f64,
    current_rate: f64,
    total_error: f64,
    greedy: bool,
    iterations: IterationSteps,
    debug_force_step: Option<usize>,
}

impl IterationCount {
    pub fn new() -> Self {
        Self {
            last_tick: Instant::now(),
            target_rate: 10.0,
            current_rate: 10.0,
            total_error: 0.0,
            greedy: false,
            iterations: IterationSteps::SleepSecs(0.0),
            debug_force_step: None,
        }
    }
    pub fn set_greedy(&mut self, greed: bool) {
        self.greedy = greed;
    }
    pub fn reset(&mut self) {
        self.current_rate = self.target_rate;
        self.total_error = 0.0;
        self.last_tick = Instant::now();
    }
    pub fn debug_change_step(&mut self, steps: Option<usize>) {
        self.debug_force_step = steps;
    }
    pub fn change_speed(&mut self, times: f64) {
        assert!(times >= 0.0);
        self.current_rate = times;
        self.target_rate = times;
    }
    pub fn update(&mut self) {
        if let Some(steps) = self.debug_force_step {
            self.iterations = IterationSteps::Debug(steps);
            self.debug_force_step = Some(0);
            self.reset();
            return;
        }
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f64() * 1000.0;
        self.last_tick = now;

        self.total_error += dt;
        let time = 1000.0 / self.current_rate;
        let steps = self.total_error / time;
        self.total_error %= time;

        if self.greedy {
            if steps > 0.99 {
                self.iterations = IterationSteps::Iterations(1);
                self.total_error -= 1;
            } else {
                let sleep = time - total_error;
                self.iterations = IterationSteps::SleepSecs(sleep);
            }
            return;
        }

        // Dont pressure cpu too much if simulation time is very high. Need to sumbit pixels on time
        let steps = steps.min(self.current_rate * 4.0);
        let maintainable = 1000.0 / dt;
        let tick_change = 8f64.min(0f64.max(self.current_rate - maintainable) * 0.2);
        if dt > 2.0 * time {
            self.current_rate = (self.current_rate - tick_change).max(self.target_rate / 4.0);
        } else {
            self.current_rate = (self.current_rate + tick_change).min(self.target_rate);
        }
        if steps as usize > 0 {
            self.iterations = IterationSteps::Iterations(steps as usize)
        } else {
            let time_sleep_millis = (1000.0 / self.current_rate - self.total_error).max(0.0);
            self.iterations = IterationSteps::SleepSecs(time_sleep_millis / 1000.0)
        }
    }
    pub fn iterations(&self) -> IterationSteps {
        self.iterations
    }
}
