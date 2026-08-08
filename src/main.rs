use anyhow::{Result, anyhow};
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

    #[arg(long)]
    compile_check: bool,

    #[arg(long)]
    test_check: bool,

    #[arg(long)]
    ignore_whitespace: bool,

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

    if let Some(m) = cli.mode {
        config.mode_override = match m.to_lowercase().as_str() {
            "mode1" => Some(ApplicationMode::Mode1),
            "mode2" => Some(ApplicationMode::Mode2),
            "mode3" => Some(ApplicationMode::Mode3),
            "mode4" => Some(ApplicationMode::Mode4),
            _ => None,
        };
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
