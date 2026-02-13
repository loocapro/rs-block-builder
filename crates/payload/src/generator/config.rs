use builder_primitives::payload::PayloadConfig;
use reth::primitives::constants::SLOT_DURATION;

use std::time::Duration;

/// Job Generator Config used to build new jobs
#[derive(Debug, Clone)]
pub struct JobGeneratorConfig {
    /// The interval at which the job should build a new payload after the last.
    pub interval: Duration,
    /// The deadline for when the payload builder job should resolve.
    pub deadline: Duration,
    /// Maximum number of tasks to spawn for building a payload.
    pub max_payload_tasks: usize,
    /// Static payload configuration
    pub payload_config: PayloadConfig,
}

// === impl JobGeneratorConfig ===

impl JobGeneratorConfig {
    /// Sets the interval at which the job should build a new payload after the last.
    pub fn interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Sets the deadline when this job should resolve.
    pub fn deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// Sets the maximum number of tasks to spawn for building a payload(s).
    ///
    /// # Panics
    ///
    /// If `max_payload_tasks` is 0.
    pub fn max_payload_tasks(mut self, max_payload_tasks: usize) -> Self {
        assert!(
            max_payload_tasks > 0,
            "max_payload_tasks must be greater than 0"
        );
        self.max_payload_tasks = max_payload_tasks;
        self
    }

    /// Sets the static payload configuration.
    pub fn payload_config(mut self, payload_config: PayloadConfig) -> Self {
        self.payload_config = payload_config;
        self
    }
}

impl Default for JobGeneratorConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            // 12s slot time
            deadline: SLOT_DURATION,
            max_payload_tasks: 3,
            payload_config: PayloadConfig::default(),
        }
    }
}
