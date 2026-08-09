use super::core::{RefineError, Result};
use crate::models::{Diagnostic, DiagnosticCategory, DiagnosticLevel, SemanticChecksConfig};
use std::{
    process::{Command, Stdio},
    time::Duration,
};
use wait_timeout::ChildExt;

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
    fn name(&self) -> &str {
        "compile"
    }
    fn check(
        &self,
        _original: &str,
        _patched: &str,
        config: &SemanticChecksConfig,
    ) -> Vec<Diagnostic> {
        if !config.run_compile_check {
            return vec![];
        }
        let cmd = "cargo build".to_string();
        let cmd = config.compile_command.as_ref().unwrap_or(&cmd);
        let timeout = config.timeout_secs.unwrap_or(30);
        execute_command_with_timeout(cmd, timeout)
    }
}

fn execute_command(cmd: &str, timeout_secs: u64) -> Result<(bool, String)> {
    // Parse cmd into program + args (simple split, or use `shell-words` crate)
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err(RefineError::EmptyCommand);
    }

    let mut child = Command::new(parts[0])
        .args(&parts[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let timeout = Duration::from_secs(timeout_secs);
    match child.wait_timeout(timeout) {
        Ok(Some(status)) => {
            let output = child.wait_with_output().unwrap();
            Ok((
                status.success(),
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
        res => {
            let _ = child.kill();
            res?;
            Err(RefineError::CommandTimedOut)
        }
    }
}

fn execute_command_with_timeout(cmd: &str, timeout: u64) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    match execute_command(cmd, timeout) {
        Ok((success, output)) => {
            if !success {
                diags.push(Diagnostic {
                    level: DiagnosticLevel::Error,
                    category: DiagnosticCategory::Compile,
                    message: output,
                    location: None,
                })
            }
        }
        Err(e) => diags.push(Diagnostic {
            level: DiagnosticLevel::Error,
            category: DiagnosticCategory::Compile,
            message: format!("Compile check error: {}", e),
            location: None,
        }),
    }
    diags
}

pub struct TestChecker;
impl SemanticChecker for TestChecker {
    fn name(&self) -> &str {
        "test"
    }
    fn check(
        &self,
        _original: &str,
        _patched: &str,
        config: &SemanticChecksConfig,
    ) -> Vec<Diagnostic> {
        if !config.run_tests {
            return vec![];
        }
        let cmd = "cargo test".to_string();
        let cmd = config.test_command.as_ref().unwrap_or(&cmd);
        let timeout = config.timeout_secs.unwrap_or(30);
        execute_command_with_timeout(cmd, timeout)
    }
}
