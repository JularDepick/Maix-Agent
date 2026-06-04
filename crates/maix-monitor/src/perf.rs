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

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_perf_collector_basic() {
        let mut collector = PerfCollector::new();
        collector.start();
        thread::sleep(Duration::from_millis(10));
        collector.sample("step1");
        assert_eq!(collector.samples().len(), 1);
        assert_eq!(collector.samples()[0].label, "step1");
        assert!(collector.samples()[0].duration >= Duration::from_millis(10));
    }

    #[test]
    fn test_perf_collector_multiple_samples() {
        let mut collector = PerfCollector::new();
        collector.start();
        thread::sleep(Duration::from_millis(5));
        collector.sample("first");
        thread::sleep(Duration::from_millis(5));
        collector.sample("second");
        assert_eq!(collector.samples().len(), 2);
        assert!(collector.total_duration() >= Duration::from_millis(10));
    }

    #[test]
    fn test_perf_collector_no_start() {
        let mut collector = PerfCollector::new();
        collector.sample("no-op");
        assert_eq!(collector.samples().len(), 0);
    }

    #[test]
    fn test_perf_collector_clear() {
        let mut collector = PerfCollector::new();
        collector.start();
        collector.sample("step");
        assert_eq!(collector.samples().len(), 1);
        collector.clear();
        assert_eq!(collector.samples().len(), 0);
        assert_eq!(collector.total_duration(), Duration::default());
    }

    #[test]
    fn test_perf_collector_new_initial_state() {
        let collector = PerfCollector::new();
        assert!(collector.samples().is_empty());
        assert_eq!(collector.total_duration(), Duration::default());
    }

    #[test]
    fn test_perf_collector_clear_then_reuse() {
        let mut collector = PerfCollector::new();
        collector.start();
        collector.sample("first");
        collector.clear();
        // Reuse after clear
        collector.start();
        collector.sample("second");
        assert_eq!(collector.samples().len(), 1);
        assert_eq!(collector.samples()[0].label, "second");
    }

    #[test]
    fn test_perf_collector_empty_label() {
        let mut collector = PerfCollector::new();
        collector.start();
        collector.sample("");
        assert_eq!(collector.samples().len(), 1);
        assert_eq!(collector.samples()[0].label, "");
    }
}
