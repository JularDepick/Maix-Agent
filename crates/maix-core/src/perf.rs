//! Performance dashboard — latency, token rate, and metric tracking.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Performance metric entry.
#[derive(Debug, Clone)]
pub struct MetricEntry {
    pub duration: Duration,
    pub timestamp: Instant,
}

/// Performance dashboard for tracking operation metrics.
pub struct PerfDashboard {
    metrics: HashMap<String, Vec<MetricEntry>>,
    token_rates: Vec<f64>,
}

impl PerfDashboard {
    pub fn new() -> Self {
        Self {
            metrics: HashMap::new(),
            token_rates: Vec::new(),
        }
    }

    pub fn record(&mut self, category: &str, duration: Duration) {
        self.metrics
            .entry(category.to_string())
            .or_default()
            .push(MetricEntry {
                duration,
                timestamp: Instant::now(),
            });
    }

    pub fn record_token_rate(&mut self, tokens: usize, duration: Duration) {
        if !duration.is_zero() {
            let rate = tokens as f64 / duration.as_secs_f64();
            self.token_rates.push(rate);
        }
    }

    pub fn categories(&self) -> Vec<&str> {
        self.metrics.keys().map(|s| s.as_str()).collect()
    }

    pub fn entry_count(&self, category: &str) -> usize {
        self.metrics.get(category).map(|v| v.len()).unwrap_or(0)
    }

    pub fn total_entries(&self) -> usize {
        self.metrics.values().map(|v| v.len()).sum()
    }

    pub fn average_duration(&self, category: &str) -> Option<Duration> {
        let entries = self.metrics.get(category)?;
        if entries.is_empty() {
            return None;
        }
        let total: Duration = entries.iter().map(|e| e.duration).sum();
        Some(total / entries.len() as u32)
    }

    pub fn min_duration(&self, category: &str) -> Option<Duration> {
        self.metrics
            .get(category)?
            .iter()
            .map(|e| e.duration)
            .min()
    }

    pub fn max_duration(&self, category: &str) -> Option<Duration> {
        self.metrics
            .get(category)?
            .iter()
            .map(|e| e.duration)
            .max()
    }

    pub fn p95_duration(&self, category: &str) -> Option<Duration> {
        let entries = self.metrics.get(category)?;
        if entries.is_empty() {
            return None;
        }
        let mut durations: Vec<Duration> = entries.iter().map(|e| e.duration).collect();
        durations.sort();
        let idx = (durations.len() as f64 * 0.95) as usize;
        durations.get(idx).copied()
    }

    pub fn average_token_rate(&self) -> f64 {
        if self.token_rates.is_empty() {
            return 0.0;
        }
        self.token_rates.iter().sum::<f64>() / self.token_rates.len() as f64
    }

    pub fn render(&self) -> String {
        let mut output = String::from("=== Performance Dashboard ===\n\n");

        output.push_str("--- Operation Latency ---\n");
        for (name, entries) in &self.metrics {
            if entries.is_empty() {
                continue;
            }
            let avg = self.average_duration(name).unwrap_or_default();
            let min = self.min_duration(name).unwrap_or_default();
            let max = self.max_duration(name).unwrap_or_default();
            let p95 = self.p95_duration(name).unwrap_or_default();
            output.push_str(&format!(
                "  {}: avg={:.0}ms min={}ms max={}ms p95={}ms n={}\n",
                name,
                avg.as_millis(),
                min.as_millis(),
                max.as_millis(),
                p95.as_millis(),
                entries.len()
            ));
        }

        if !self.token_rates.is_empty() {
            output.push_str(&format!(
                "\n--- Token Rate ---\n  avg: {:.1} tokens/sec\n",
                self.average_token_rate()
            ));
        }

        output
    }

    pub fn clear(&mut self) {
        self.metrics.clear();
        self.token_rates.clear();
    }
}

impl Default for PerfDashboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_new() {
        let d = PerfDashboard::new();
        assert!(d.categories().is_empty());
    }

    #[test]
    fn test_record_metric() {
        let mut d = PerfDashboard::new();
        d.record("llm", Duration::from_millis(100));
        d.record("llm", Duration::from_millis(200));
        assert_eq!(d.entry_count("llm"), 2);
    }

    #[test]
    fn test_average_duration() {
        let mut d = PerfDashboard::new();
        d.record("test", Duration::from_millis(100));
        d.record("test", Duration::from_millis(200));
        let avg = d.average_duration("test").unwrap();
        assert_eq!(avg, Duration::from_millis(150));
    }

    #[test]
    fn test_min_max_duration() {
        let mut d = PerfDashboard::new();
        d.record("test", Duration::from_millis(50));
        d.record("test", Duration::from_millis(200));
        assert_eq!(d.min_duration("test").unwrap(), Duration::from_millis(50));
        assert_eq!(d.max_duration("test").unwrap(), Duration::from_millis(200));
    }

    #[test]
    fn test_p95_duration() {
        let mut d = PerfDashboard::new();
        for i in 1..=100 {
            d.record("test", Duration::from_millis(i));
        }
        let p95 = d.p95_duration("test").unwrap();
        assert!(p95 >= Duration::from_millis(94));
    }

    #[test]
    fn test_token_rate() {
        let mut d = PerfDashboard::new();
        d.record_token_rate(100, Duration::from_secs(2));
        assert!((d.average_token_rate() - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_render() {
        let mut d = PerfDashboard::new();
        d.record("llm", Duration::from_millis(500));
        let report = d.render();
        assert!(report.contains("Performance Dashboard"));
        assert!(report.contains("llm"));
    }

    #[test]
    fn test_empty_category() {
        let d = PerfDashboard::new();
        assert_eq!(d.entry_count("none"), 0);
        assert!(d.average_duration("none").is_none());
    }

    #[test]
    fn test_total_entries() {
        let mut d = PerfDashboard::new();
        d.record("a", Duration::from_millis(1));
        d.record("b", Duration::from_millis(2));
        assert_eq!(d.total_entries(), 2);
    }

    #[test]
    fn test_clear() {
        let mut d = PerfDashboard::new();
        d.record("test", Duration::from_millis(1));
        d.clear();
        assert!(d.categories().is_empty());
    }
}
