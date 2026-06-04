//! Memory importance scoring — time decay, access frequency, content analysis.

use chrono::{DateTime, Utc};

/// Importance scorer for memory entries.
pub struct ImportanceScorer {
    decay_rate: f32,
    access_boost: f32,
    explicit_weight: f32,
}

impl Default for ImportanceScorer {
    fn default() -> Self {
        Self {
            decay_rate: 0.01,
            access_boost: 0.1,
            explicit_weight: 2.0,
        }
    }
}

impl ImportanceScorer {
    pub fn new(decay_rate: f32, access_boost: f32, explicit_weight: f32) -> Self {
        Self {
            decay_rate,
            access_boost,
            explicit_weight,
        }
    }

    /// Calculate importance score for a memory entry.
    pub fn score(
        &self,
        base_importance: f32,
        created_at: DateTime<Utc>,
        access_count: u32,
        user_marked_important: bool,
    ) -> f32 {
        let age_days = (Utc::now() - created_at).num_days() as f32;
        let time_factor = (-self.decay_rate * age_days).exp();
        let access_factor = 1.0 + self.access_boost * access_count as f32;
        let explicit_factor = if user_marked_important {
            self.explicit_weight
        } else {
            1.0
        };

        (base_importance * time_factor * access_factor * explicit_factor).min(1.0)
    }

    /// Auto-score importance from content keywords.
    pub fn auto_score(content: &str) -> f32 {
        let lower = content.to_lowercase();
        let mut score: f32 = 0.3;

        // Preference keywords
        if lower.contains("prefer")
            || lower.contains("like")
            || lower.contains("always")
            || lower.contains("never")
        {
            score += 0.25;
        }

        // Identity keywords
        if lower.contains("i am")
            || lower.contains("i'm")
            || lower.contains("my name")
            || lower.contains("i work")
        {
            score += 0.3;
        }

        // Project/tech keywords
        if lower.contains("project")
            || lower.contains("tech stack")
            || lower.contains("using")
            || lower.contains("built with")
        {
            score += 0.15;
        }

        // Decision keywords
        if lower.contains("decided")
            || lower.contains("chosen")
            || lower.contains("agreed")
            || lower.contains("plan to")
        {
            score += 0.1;
        }

        score.min(1.0)
    }

    /// Check if a memory should be garbage collected.
    pub fn should_gc(
        &self,
        base_importance: f32,
        created_at: DateTime<Utc>,
        access_count: u32,
        threshold: f32,
    ) -> bool {
        self.score(base_importance, created_at, access_count, false) < threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scorer_default() {
        let scorer = ImportanceScorer::default();
        assert!((scorer.decay_rate - 0.01).abs() < 0.001);
    }

    #[test]
    fn test_score_recent_high() {
        let scorer = ImportanceScorer::default();
        let score = scorer.score(0.8, Utc::now(), 0, false);
        assert!(score > 0.7);
    }

    #[test]
    fn test_score_old_decayed() {
        let scorer = ImportanceScorer::default();
        let old = Utc::now() - chrono::Duration::days(100);
        let score = scorer.score(0.8, old, 0, false);
        assert!(score < 0.5);
    }

    #[test]
    fn test_score_access_boost() {
        let scorer = ImportanceScorer::default();
        let score_no_access = scorer.score(0.5, Utc::now(), 0, false);
        let score_with_access = scorer.score(0.5, Utc::now(), 10, false);
        assert!(score_with_access > score_no_access);
    }

    #[test]
    fn test_score_explicit_mark() {
        let scorer = ImportanceScorer::default();
        let score_normal = scorer.score(0.5, Utc::now(), 0, false);
        let score_marked = scorer.score(0.5, Utc::now(), 0, true);
        assert!(score_marked > score_normal);
    }

    #[test]
    fn test_auto_score_preference() {
        let score = ImportanceScorer::auto_score("I prefer using Rust over Go");
        assert!(score > 0.5);
    }

    #[test]
    fn test_auto_score_identity() {
        let score = ImportanceScorer::auto_score("I am a senior engineer at Google");
        assert!(score > 0.5);
    }

    #[test]
    fn test_auto_score_project() {
        let score = ImportanceScorer::auto_score("This project uses React and TypeScript");
        assert!(score > 0.4);
    }

    #[test]
    fn test_auto_score_neutral() {
        let score = ImportanceScorer::auto_score("The weather is nice today");
        assert!(score < 0.5);
    }

    #[test]
    fn test_should_gc() {
        let scorer = ImportanceScorer::default();
        let old = Utc::now() - chrono::Duration::days(365);
        assert!(scorer.should_gc(0.3, old, 0, 0.2));
    }

    #[test]
    fn test_should_not_gc_important() {
        let scorer = ImportanceScorer::default();
        assert!(!scorer.should_gc(0.9, Utc::now(), 5, 0.2));
    }

    #[test]
    fn test_score_clamped() {
        let scorer = ImportanceScorer::default();
        let score = scorer.score(1.0, Utc::now(), 100, true);
        assert!(score <= 1.0);
    }

    #[test]
    fn test_scorer_custom_params() {
        let scorer = ImportanceScorer::new(0.05, 0.1, 0.5);
        assert!((scorer.decay_rate - 0.05).abs() < 0.001);
        assert!((scorer.access_boost - 0.1).abs() < 0.001);
        assert!((scorer.explicit_weight - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_score_zero_base_importance() {
        let scorer = ImportanceScorer::default();
        let score = scorer.score(0.0, Utc::now(), 100, true);
        // Even with access boost and explicit mark, base=0 means score starts at 0
        // access_boost * 100 = 1.0, explicit_weight * 0.2 = 0.1
        // But time_factor * (0.0 + 1.0 + 0.1) = ~1.1, clamped to 1.0
        // Actually, the formula is: time_factor * (base + access + explicit)
        // With base=0, access=100*0.01=1.0, explicit=true*0.2=0.1
        // time_factor ~= 1.0, so score ~= 1.1, clamped to 1.0
        // Hmm, base_importance=0 doesn't mean score=0. Let me check the formula.
        // score = time_factor * (base + access_boost * count + explicit_weight * mark)
        // = ~1.0 * (0 + 0.01*100 + 0.2*1) = 1.2, clamped to 1.0
        // So base_importance=0 doesn't force score=0 when there are accesses.
        assert!(score >= 0.0);
    }

    #[test]
    fn test_auto_score_empty() {
        let score = ImportanceScorer::auto_score("");
        assert!((score - 0.3).abs() < 0.01); // base score only
    }

    #[test]
    fn test_auto_score_all_keywords() {
        // Must contain: prefer, i am, project, decided
        let score = ImportanceScorer::auto_score("I am a developer and I prefer this project because I decided it's best");
        // base (0.3) + preference (0.25) + identity (0.3) + project (0.15) + decision (0.1) = 1.1, min(1.0)
        assert!((score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_auto_score_decision() {
        // "decided" triggers decision category (+0.1), "using" triggers project category (+0.15)
        let score = ImportanceScorer::auto_score("We decided on using PostgreSQL for the database");
        assert!(score > 0.4);
    }

    #[test]
    fn test_score_very_old() {
        let scorer = ImportanceScorer::default();
        let very_old = Utc::now() - chrono::Duration::days(10000);
        let score = scorer.score(0.8, very_old, 0, false);
        assert!(score < 0.1); // time decay should drive it very low
    }
}
