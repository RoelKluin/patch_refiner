use crate::models::{Diagnostic, SemanticChecksConfig};

pub trait SemanticChecker {
    fn name(&self) -> &str;
    fn check(
        &self,
        original_code: &str,
        patched_code: &str,
        config: &SemanticChecksConfig,
    ) -> Vec<Diagnostic>;
}

pub struct CompileChecker;
impl SemanticChecker for CompileChecker {
    fn name(&self) -> &str { "compile" }
    fn check(&self, _original: &str, _patched: &str, config: &SemanticChecksConfig) -> Vec<Diagnostic> {
        if !config.run_compile_check { return vec![]; }
        // TODO: Implement actual subprocess compilation logic based on config.compile_command
        vec![] 
    }
}
