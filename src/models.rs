use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementRequest {
    pub schema_version: Option<String>,
    pub original_code: String,
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
    pub similarity: SimilarityConfig,
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
    pub fn validate(&self) -> Result<(), String> {
        if let Some(timeout) = self.timeout_secs {
            if timeout == 0 {
                return Err("timeout_secs must be > 0".to_string());
            }
        }
        if self.run_compile_check && self.compile_command.is_none() {
            return Err("run_compile_check=true requires compile_command".to_string());
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
pub struct SimilarityConfig {
    pub add_weight: f64,
    pub del_weight: f64,
    pub mod_weight: f64,
    pub ignore_comments: bool,
}

impl SimilarityConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.add_weight < 0.0 || self.del_weight < 0.0 || self.mod_weight < 0.0 {
            return Err("Weights must be non-negative".to_string());
        }
        Ok(())
    }
}

impl Default for SimilarityConfig {
    fn default() -> Self {
        Self {
            add_weight: 1.0,
            del_weight: 1.0,
            mod_weight: 1.5,
            ignore_comments: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhitespaceConfig {
    pub ignore_whitespace: bool,
    pub normalize_line_endings: bool,
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
    pub target_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfectPatch {
    pub id: String,
    pub diff_content: String,
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
    Mode1,
    Mode2,
    Mode3,
    Mode4,
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
    pub distance_score: usize,
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
