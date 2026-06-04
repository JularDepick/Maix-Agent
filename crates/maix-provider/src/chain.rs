//! Provider fallback chain — automatic failover between providers.


/// A provider entry in the fallback chain.
pub struct ProviderEntry {
    pub name: String,
    pub model: String,
    pub priority: u8,
    pub enabled: bool,
}

/// Chain of providers with automatic fallback.
pub struct ProviderChain {
    entries: Vec<ProviderEntry>,
    current_index: usize,
}

impl ProviderChain {
    pub fn new(entries: Vec<ProviderEntry>) -> Self {
        let mut sorted = entries;
        sorted.sort_by_key(|e| e.priority);
        Self {
            entries: sorted,
            current_index: 0,
        }
    }

    pub fn current(&self) -> Option<&ProviderEntry> {
        self.entries.get(self.current_index).filter(|e| e.enabled)
    }

    pub fn current_name(&self) -> &str {
        self.current()
            .map(|e| e.name.as_str())
            .unwrap_or("none")
    }

    pub fn current_model(&self) -> &str {
        self.current()
            .map(|e| e.model.as_str())
            .unwrap_or("none")
    }

    /// Advance to the next provider in the chain.
    pub fn fallback(&mut self) -> bool {
        let start = self.current_index;
        loop {
            self.current_index = (self.current_index + 1) % self.entries.len();
            if self.current_index == start {
                return false; // full cycle
            }
            if self.entries[self.current_index].enabled {
                return true;
            }
        }
    }

    /// Reset to the highest priority provider.
    pub fn reset(&mut self) {
        self.current_index = 0;
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn enabled_count(&self) -> usize {
        self.entries.iter().filter(|e| e.enabled).count()
    }

    pub fn disable_current(&mut self) {
        if let Some(entry) = self.entries.get_mut(self.current_index) {
            entry.enabled = false;
        }
    }

    pub fn enable_all(&mut self) {
        for entry in &mut self.entries {
            entry.enabled = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entries() -> Vec<ProviderEntry> {
        vec![
            ProviderEntry {
                name: "openai".into(),
                model: "gpt-4o".into(),
                priority: 1,
                enabled: true,
            },
            ProviderEntry {
                name: "anthropic".into(),
                model: "claude-sonnet-4-20250514".into(),
                priority: 2,
                enabled: true,
            },
            ProviderEntry {
                name: "deepseek".into(),
                model: "deepseek-chat".into(),
                priority: 3,
                enabled: true,
            },
        ]
    }

    #[test]
    fn test_chain_new() {
        let chain = ProviderChain::new(sample_entries());
        assert_eq!(chain.entry_count(), 3);
        assert_eq!(chain.current_name(), "openai");
    }

    #[test]
    fn test_chain_fallback() {
        let mut chain = ProviderChain::new(sample_entries());
        assert_eq!(chain.current_name(), "openai");
        assert!(chain.fallback());
        assert_eq!(chain.current_name(), "anthropic");
        assert!(chain.fallback());
        assert_eq!(chain.current_name(), "deepseek");
    }

    #[test]
    fn test_chain_fallback_wraps() {
        let entries = vec![
            ProviderEntry {
                name: "a".into(),
                model: "m1".into(),
                priority: 1,
                enabled: true,
            },
            ProviderEntry {
                name: "b".into(),
                model: "m2".into(),
                priority: 2,
                enabled: true,
            },
        ];
        let mut chain = ProviderChain::new(entries);
        assert!(chain.fallback()); // a -> b
        assert!(chain.fallback()); // b -> a (wrap)
        assert_eq!(chain.current_name(), "a");
    }

    #[test]
    fn test_chain_reset() {
        let mut chain = ProviderChain::new(sample_entries());
        chain.fallback();
        chain.fallback();
        chain.reset();
        assert_eq!(chain.current_name(), "openai");
    }

    #[test]
    fn test_chain_disable_current() {
        let mut chain = ProviderChain::new(sample_entries());
        chain.disable_current();
        assert_eq!(chain.enabled_count(), 2);
        // Current is now disabled, fallback should skip it
        chain.fallback();
        assert_eq!(chain.current_name(), "anthropic");
    }

    #[test]
    fn test_chain_enable_all() {
        let mut chain = ProviderChain::new(sample_entries());
        chain.disable_current();
        chain.enable_all();
        assert_eq!(chain.enabled_count(), 3);
    }

    #[test]
    fn test_chain_sorted_by_priority() {
        let entries = vec![
            ProviderEntry {
                name: "low".into(),
                model: "m".into(),
                priority: 10,
                enabled: true,
            },
            ProviderEntry {
                name: "high".into(),
                model: "m".into(),
                priority: 1,
                enabled: true,
            },
        ];
        let chain = ProviderChain::new(entries);
        assert_eq!(chain.current_name(), "high");
    }

    #[test]
    fn test_chain_single_entry() {
        let entries = vec![ProviderEntry {
            name: "only".into(),
            model: "m".into(),
            priority: 1,
            enabled: true,
        }];
        let mut chain = ProviderChain::new(entries);
        assert!(!chain.fallback()); // can't fallback
    }

    #[test]
    fn test_chain_current_model() {
        let chain = ProviderChain::new(sample_entries());
        assert_eq!(chain.current_model(), "gpt-4o");
    }

    #[test]
    fn test_chain_current_model_none_when_all_disabled() {
        let entries = vec![ProviderEntry {
            name: "a".into(),
            model: "m1".into(),
            priority: 1,
            enabled: false,
        }];
        let chain = ProviderChain::new(entries);
        assert_eq!(chain.current_model(), "none");
        assert_eq!(chain.current_name(), "none");
        assert!(chain.current().is_none());
    }
}
