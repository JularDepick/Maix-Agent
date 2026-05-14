//! Self-reflection — agent evaluates and refines its own responses.

use serde::{Deserialize, Serialize};

/// Result of a reflection evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionResult {
    pub quality_score: f32,
    pub issues: Vec<String>,
    pub suggestion: Option<String>,
    pub revised_response: Option<String>,
}

/// Self-reflection engine.
pub struct SelfReflector {
    min_confidence: f32,
    max_retries: usize,
}

impl SelfReflector {
    pub fn new(min_confidence: f32, max_retries: usize) -> Self {
        Self {
            min_confidence: min_confidence.clamp(0.0, 1.0),
            max_retries,
        }
    }

    pub fn min_confidence(&self) -> f32 {
        self.min_confidence
    }

    pub fn max_retries(&self) -> usize {
        self.max_retries
    }

    /// Evaluate a response quality. Returns a score 0.0-1.0.
    pub fn evaluate_response(&self, response: &str) -> ReflectionResult {
        let mut score: f32 = 0.5;
        let mut issues = Vec::new();

        // Length check
        if response.is_empty() {
            return ReflectionResult {
                quality_score: 0.0,
                issues: vec!["Empty response".to_string()],
                suggestion: Some("Provide a substantive answer".to_string()),
                revised_response: None,
            };
        }

        if response.len() < 10 {
            score -= 0.2;
            issues.push("Response too short".to_string());
        }

        // Check for uncertainty markers
        let lower = response.to_lowercase();
        if lower.contains("i'm not sure") || lower.contains("i don't know") {
            score -= 0.1;
            issues.push("Contains uncertainty markers".to_string());
        }

        // Check for code blocks (good for coding tasks)
        if response.contains("```") {
            score += 0.15;
        }

        // Check for structured content
        if response.contains("\n- ") || response.contains("\n* ") || response.contains("\n1.") {
            score += 0.1;
        }

        // Check for very long responses (might be verbose)
        if response.len() > 5000 {
            score -= 0.05;
            issues.push("Response may be too verbose".to_string());
        }

        let suggestion = if !issues.is_empty() {
            Some(format!("Consider: {}", issues.join("; ")))
        } else {
            None
        };

        ReflectionResult {
            quality_score: score.clamp(0.0, 1.0),
            issues,
            suggestion,
            revised_response: None,
        }
    }

    /// Check if a response needs refinement.
    pub fn needs_refinement(&self, result: &ReflectionResult) -> bool {
        result.quality_score < self.min_confidence
    }

    /// Build a refinement prompt for the LLM.
    pub fn refinement_prompt(&self, original: &str, reflection: &ReflectionResult) -> String {
        let issues = reflection.issues.join(", ");
        let suggestion = reflection.suggestion.as_deref().unwrap_or("improve quality");
        format!(
            "Please improve the following response.\nIssues: {}\nSuggestion: {}\n\nOriginal:\n{}",
            issues, suggestion, original
        )
    }
}

impl Default for SelfReflector {
    fn default() -> Self {
        Self::new(0.6, 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reflector_default() {
        let r = SelfReflector::default();
        assert!((r.min_confidence() - 0.6).abs() < 0.01);
        assert_eq!(r.max_retries(), 2);
    }

    #[test]
    fn test_evaluate_empty() {
        let r = SelfReflector::default();
        let result = r.evaluate_response("");
        assert!((result.quality_score).abs() < 0.01);
        assert!(!result.issues.is_empty());
    }

    #[test]
    fn test_evaluate_short() {
        let r = SelfReflector::default();
        let result = r.evaluate_response("yes");
        assert!(result.quality_score < 0.5);
    }

    #[test]
    fn test_evaluate_good_code() {
        let r = SelfReflector::default();
        let response = "Here's the solution:\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\nThis does X and Y.";
        let result = r.evaluate_response(response);
        assert!(result.quality_score > 0.5);
    }

    #[test]
    fn test_evaluate_structured() {
        let r = SelfReflector::default();
        let response = "Steps:\n- First do this\n- Then do that\n- Finally verify";
        let result = r.evaluate_response(response);
        assert!(result.quality_score >= 0.5);
    }

    #[test]
    fn test_needs_refinement() {
        let r = SelfReflector::new(0.7, 2);
        let low = ReflectionResult {
            quality_score: 0.3,
            issues: vec![],
            suggestion: None,
            revised_response: None,
        };
        assert!(r.needs_refinement(&low));

        let high = ReflectionResult {
            quality_score: 0.9,
            issues: vec![],
            suggestion: None,
            revised_response: None,
        };
        assert!(!r.needs_refinement(&high));
    }

    #[test]
    fn test_refinement_prompt() {
        let r = SelfReflector::default();
        let reflection = ReflectionResult {
            quality_score: 0.3,
            issues: vec!["Too short".into()],
            suggestion: Some("Add more detail".into()),
            revised_response: None,
        };
        let prompt = r.refinement_prompt("short answer", &reflection);
        assert!(prompt.contains("Too short"));
        assert!(prompt.contains("short answer"));
    }

    #[test]
    fn test_reflection_serialize() {
        let result = ReflectionResult {
            quality_score: 0.8,
            issues: vec!["a".into()],
            suggestion: None,
            revised_response: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("0.8"));
    }

    #[test]
    fn test_uncertainty_markers() {
        let r = SelfReflector::default();
        let result = r.evaluate_response("I'm not sure about this, but maybe it works somehow in some cases");
        assert!(result.issues.iter().any(|i| i.contains("uncertainty")));
    }
}
