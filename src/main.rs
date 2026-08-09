use anyhow::{anyhow, Result};
use clap::Parser;
use std::fs;
use std::io::{self, Read};

use patch_refiner::core::PatchRefiner;
use patch_refiner::models::{ApplicationMode, RefinementRequest};

#[derive(Parser, Debug)]
#[command(author, version, about = "AI Patch-Refinement Module for APR")]
struct Cli {
    #[arg(short, long)]
    input: Option<String>,

    #[arg(long, value_name = "MODE")]
    mode: Option<String>,

    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    compile_check: Option<bool>,

    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    test_check: Option<bool>,

    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    ignore_whitespace: Option<bool>,

    #[arg(long)]
    language: Option<String>,
}

pub fn default_commands(language: &str) -> (String, String) {
    match language {
        "rust" => ("cargo build".into(), "cargo test".into()),
        "bash" => ("bash -n".into(), "bats *.bats".into()), // syntax check + bats tests
        "markdown" => ("true".into(), "mdl .".into()),      // markdown-lint
        _ => ("".into(), "".into()),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let input_data = match cli.input {
        Some(path) => fs::read_to_string(path)?,
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            buf
        }
    };
    // FIXME: what to do with these linters?
    let _default_commands = default_commands(cli.language.as_deref().unwrap_or(""));

    let mut request: RefinementRequest = serde_json::from_str(&input_data)
        .map_err(|e| anyhow!("Failed to parse JSON request:\n{e}"))?;

    let mut config = request.config.unwrap_or_default();
    if let Some(v) = cli.compile_check {
        config.semantic_checks.run_compile_check = v;
    }
    if let Some(v) = cli.test_check {
        config.semantic_checks.run_tests = v;
    }
    if let Some(v) = cli.ignore_whitespace {
        config.whitespace.ignore_whitespace = v;
    }

    let effective_lang = cli
        .language
        .as_deref()
        .or(config.language.as_deref())
        .unwrap_or("");
    let (default_compile, default_test) = default_commands(effective_lang);
    if config.semantic_checks.run_compile_check && config.semantic_checks.compile_command.is_none()
    {
        config.semantic_checks.compile_command = Some(default_compile);
    }
    if config.semantic_checks.run_tests && config.semantic_checks.test_command.is_none() {
        config.semantic_checks.test_command = Some(default_test);
    }

    if let Some(m) = cli.mode {
        config.mode_override = Some(match m.to_lowercase().as_str() {
            "mode1" => ApplicationMode::Mode1,
            "mode2" => ApplicationMode::Mode2,
            "mode3" => ApplicationMode::Mode3,
            "mode4" => ApplicationMode::Mode4,
            other => anyhow::bail!("invalid --mode value: {other}"),
        });
    }

    if cli.compile_check {
        config.semantic_checks.run_compile_check = true;
    }
    if cli.test_check {
        config.semantic_checks.run_tests = true;
    }
    if cli.ignore_whitespace {
        config.whitespace.ignore_whitespace = true;
    }

    request.config = Some(config);

    let response = PatchRefiner::evaluate(request)?;

    let output_json = serde_json::to_string_pretty(&response)?;
    println!("{}", output_json);

    Ok(())
}
