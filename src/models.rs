use super::core::{RefineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: &str = "2.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementRequest {
    pub schema_version: Option<String>,
    /// Keyed by target path (matched against `PatchCandidate`/`PerfectPatch`
    /// `target_path`), so a request can validate patches spanning multiple
    /// files in one call. Each individual diff still targets exactly one
    /// file -- see .claude/rules/patch-application.md on multi-file diffs.
    pub files: BTreeMap<String, String>,
    pub candidates: Vec<PatchCandidate>,
    pub perfect_patches: Option<Vec<PerfectPatch>>,
    pub problem_statement: Option<String>,
    pub config: Option<RefinementConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RefinementConfig {
    pub mode_override: Option<ApplicationMode>,
    pub language: Option<String>,
    pub file_path: Option<String>,
    #[serde(default)]
    pub semantic_checks: SemanticChecksConfig,
    #[serde(default)]
    pub language_weights: Option<LanguageWeights>,
    #[serde(default)]
    pub whitespace: WhitespaceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticChecksConfig {
    pub run_compile_check: bool,
    pub run_tests: bool,
    pub compile_command: Option<String>,
    pub test_command: Option<String>,
    pub timeout_secs: Option<u64>,
}
impl SemanticChecksConfig {
    pub fn validate(&self) -> Result<()> {
        if let Some(timeout) = self.timeout_secs
            && timeout == 0
        {
            return Err(RefineError::InvalidTimeout);
        }
        if self.run_compile_check && self.compile_command.is_none() {
            return Err(RefineError::MissingCompileCommand);
        }
        Ok(())
    }
}
impl Default for SemanticChecksConfig {
    fn default() -> Self {
        Self {
            run_compile_check: false,
            run_tests: false,
            compile_command: None,
            test_command: None,
            timeout_secs: Some(30),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageWeights {
    pub code_weight: f64,
    pub string_weight: f64,
    pub comment_weight: f64,
    pub string_delimiters: Vec<String>, // e.g., ["\"", "'"]
    pub comment_markers: Vec<(String, String)>, // e.g., [("//", ""), ("/*", "*/")]
}

impl Default for LanguageWeights {
    fn default() -> Self {
        // Rust defaults
        Self {
            code_weight: 1.0,
            string_weight: 0.1,
            comment_weight: 0.05,
            string_delimiters: vec!["\"".to_string()],
            comment_markers: vec![
                ("//".to_string(), "".to_string()),
                ("/*".to_string(), "*/".to_string()),
            ],
        }
    }
}

impl LanguageWeights {
    pub fn validate(&self) -> Result<()> {
        if self.code_weight <= self.string_weight {
            return Err(RefineError::InvalidLanguageWeights(
                "code_weight".to_string(),
                "string_weight".to_string(),
            ));
        }
        if self.string_weight < self.comment_weight {
            return Err(RefineError::InvalidLanguageWeights(
                "string_weight".to_string(),
                "comment_weight".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhitespaceConfig {
    pub ignore_whitespace: bool,
    pub normalize_line_endings: bool,
}

impl WhitespaceConfig {
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl Default for WhitespaceConfig {
    fn default() -> Self {
        Self {
            ignore_whitespace: true,
            normalize_line_endings: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchCandidate {
    pub id: String,
    pub diff_content: String,
    /// Key into `RefinementRequest.files` this candidate's diff applies to.
    /// Always required -- never inferred from the diff's own headers.
    pub target_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfectPatch {
    pub id: String,
    pub diff_content: String,
    /// Key into `RefinementRequest.files` this exemplar's diff applies to.
    pub target_path: String,
    pub reason: Option<Reason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reason {
    pub summary: String,
    pub details: Vec<ReasonDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasonDetail {
    pub hunk_index: Option<usize>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub text: String,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationMode {
    SyntacticOnly,
    SingleExemplar,
    MultiExemplar,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approved,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deviation {
    pub candidate_id: String,
    pub closest_perfect_patch_id: String,
    pub diff_from_perfect: String,
    pub distance_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub category: DiagnosticCategory,
    pub message: String,
    pub location: Option<SourceLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCategory {
    PatchParse,
    PatchApply,
    Compile,
    Test,
    Similarity,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementResponse {
    pub schema_version: String,
    pub mode: ApplicationMode,
    pub decision: Decision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_patch_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_perfect_patch_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deviations: Option<Deviation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reason>,
    pub diagnostics: Vec<Diagnostic>,
}
