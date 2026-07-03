// Crash simulation module
use std::process;
use std::time::Duration;

pub struct CrashSimulator {
    enabled: bool,
    crash_phase: Option<CrashPhase>,
    crash_countdown: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CrashPhase {
    DuringNormalization,
    DuringConcat,
    DuringCardRendering,
    DuringSplit,
    DuringValidation,
}

impl CrashSimulator {
    pub fn new() -> Self {
        Self {
            enabled: false,
            crash_phase: None,
            crash_countdown: None,
        }
    }

    pub fn enable(phase: CrashPhase, after_calls: u32) -> Self {
        Self {
            enabled: true,
            crash_phase: Some(phase),
            crash_countdown: Some(after_calls),
        }
    }

    pub fn should_crash(&mut self, current_phase: CrashPhase) -> bool {
        if !self.enabled {
            return false;
        }

        if self.crash_phase == Some(current_phase) {
            if let Some(countdown) = self.crash_countdown {
                if countdown == 0 {
                    return true;
                } else {
                    self.crash_countdown = Some(countdown - 1);
                }
            }
        }
        false
    }

    pub fn simulate_crash(&self) {
        eprintln!("[CRASH SIMULATION] Forcing crash exit");
        process::exit(1);
    }

    pub fn inject_delay(&self, duration: Duration) {
        if self.enabled {
            std::thread::sleep(duration);
        }
    }
}

impl Default for CrashSimulator {
    fn default() -> Self {
        Self::new()
    }
}