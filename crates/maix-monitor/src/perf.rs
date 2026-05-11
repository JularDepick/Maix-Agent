//! Performance monitoring — timing and metrics collection.

use std::time::{Duration, Instant};

/// Performance metric collector.
#[derive(Debug, Clone, Default)]
pub struct PerfCollector {
    samples: Vec<PerfSample>,
    start_time: Option<Instant>,
}

/// A single performance sample.
#[derive(Debug, Clone)]
pub struct PerfSample {
    pub label: String,
    pub duration: Duration,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl PerfCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
    }

    pub fn sample(&mut self, label: &str) {
        if let Some(start) = self.start_time {
            self.samples.push(PerfSample {
                label: label.into(),
                duration: start.elapsed(),
                timestamp: chrono::Utc::now(),
            });
        }
    }

    pub fn samples(&self) -> &[PerfSample] {
        &self.samples
    }

    pub fn total_duration(&self) -> Duration {
        self.samples
            .last()
            .map(|s| s.duration)
            .unwrap_or_default()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
        self.start_time = None;
    }
}
