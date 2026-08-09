use crate::checkers::{CompileChecker, SemanticChecker, TestChecker};
use crate::models::*;
use anyhow::{Result, anyhow, ensure};
use diffy::{Patch, apply};
use prettydiff::text::InlineChangeset;
use std::collections::HashMap;

pub struct PatchRefiner;

const MARKERS: &[(&str, &str)] = &[("//", "\n"), ("/*", "*/"), ("#\"", "\"#")];

trait RustSectionOp {
    fn is_hash_run(&self) -> bool;
    fn quote_hash_open_shape(&self) -> Option<usize>;
    fn quote_hash_close_shape(&self) -> Option<usize>;
    fn is_alphabetic(&self) -> bool;
    fn reverse(&self) -> String;
    fn get_extendable(&self) -> &str;
}

impl RustSectionOp for str {
    fn is_hash_run(&self) -> bool {
        self.chars().all(|c| c == '#')
    }
    fn quote_hash_open_shape(&self) -> Option<usize> {
        self.strip_suffix('"')
            .filter(|r| r.is_hash_run())
            .map(str::len)
    }
    fn quote_hash_close_shape(&self) -> Option<usize> {
        self.strip_prefix('"')
            .filter(|r| r.is_hash_run())
            .map(str::len)
    }
    fn is_alphabetic(&self) -> bool {
        self.chars().all(|c| c.is_alphabetic())
    }
    fn reverse(&self) -> String {
        self.chars().rev().collect()
    }
    fn get_extendable(&self) -> &str {
        match self {
            "*" => "/*",
            "/" => "//",
            "#" => "\"#",
            _ => "X", // will not match
        }
    }
}

struct Run {
    counters: HashMap<String, usize>,
    record: String,
}

impl Run {
    fn new(record: String) -> Self {
        Self {
            counters: HashMap::new(),
            record,
        }
    }
}

fn evaluate(counters: &HashMap<String, usize>) -> (usize, usize) {
    let mut expected = 0;
    let mut unexpected = 0;
    for (key, &n) in counters {
        if key.starts_with("expected:") {
            expected += n;
        } else if key.starts_with("unexpected:") {
            unexpected += n;
        }
    }
    (expected, unexpected)
}

fn hypothesis_score(closed_cleanly: bool, expected: usize, unexpected: usize) -> (bool, f64) {
    (closed_cleanly, expected as f64 / (unexpected as f64 + 1.0))
}

struct ChangeSet {
    factor: f64,
    buffer: String,
    section: String,
    line_nr: usize,
    column: usize,
    run: Vec<Run>,
    index: usize,
}

impl ChangeSet {
    fn new() -> Self {
        Self {
            factor: 0.0,
            buffer: String::new(),
            section: String::new(),
            line_nr: 0,
            column: 0,
            run: vec![Run::new(String::new())],
            index: 0,
        }
    }
    fn reset(&mut self, cfg: &LanguageWeights, i: usize) {
        self.index = i;
        self.section = self.run[i].record.clone();
        self.line_nr = 0;
        self.column = 0;
        self.factor = match self.section.as_str() {
            "" => cfg.code_weight,
            "*/" | "/*" | "//" => cfg.comment_weight,
            _ => cfg.string_weight,
        };
    }
    fn finalize(&mut self, cfg: &LanguageWeights) -> f64 {
        if self.buffer.is_empty() {
            return 0.0;
        }
        eprintln!("flushing buffer: {}", self.buffer);
        let leftover = std::mem::take(&mut self.buffer);
        self.handle_part_inner(&leftover, cfg)
    }
    fn count(&mut self, key: &str) {
        *self.run[self.index]
            .counters
            .entry(key.to_string())
            .or_insert(0) += 1;
    }
    #[cfg(test)]
    fn get_counter(&self, key: &str) -> usize {
        *self.run[self.index].counters.get(key).unwrap_or(&0)
    }
    fn counters(&self) -> &HashMap<String, usize> {
        &self.run[self.index].counters
    }
    fn handle_part_inner(&mut self, part: &str, cfg: &LanguageWeights) -> f64 {
        self.column += part.len();
        match (self.section.as_str(), part.trim()) {
            (section, "") => {
                if part == "\n" {
                    self.line_nr += 1;
                    self.column = 0;
                    if section == "//" {
                        self.section = String::new();
                        self.factor = cfg.code_weight;
                    }
                }
                return 0.0;
            }
            ("", "//" | "/*") => {
                self.section = part.to_string();
                self.factor = cfg.comment_weight;
            }
            ("", q) if q == "\"" || q.contains("#\"") => {
                self.section = q.to_string();
                self.factor = cfg.string_weight;
            }
            ("", r) if r == "*/" || r.starts_with("\"#") => {
                let rec = r.reverse();
                if !self.run.iter().any(|run| run.record == rec) {
                    self.run.push(Run::new(rec));
                }
                if r == "*/" {
                    self.count("unexpected:code_comment_end_marker")
                } else {
                    self.count("unexpected:code_rawstring_end_marker")
                }
            }
            ("", p) if p.is_alphabetic() => {
                // TODO distinguish word from identifier (use inflect?)
                self.count("unexpected:code_word")
            }
            ("", _) => self.count("expected:code_punct"),
            ("/*", "*/") | ("\"", "\"") => {
                self.section = String::new();
                self.factor = cfg.code_weight;
            }
            ("\"", p) if p.is_alphabetic() => self.count("expected:string_word"),
            ("\"", _) => {
                // TODO: distinguish acceptable punctuation here
                self.count("unexpected:string_punct")
            }
            ("//" | "/*", _) => self.count("expected:comment_content"),
            (p1, p2) if p1 != "//" && p1 != "/*" => {
                if let Some(q2) = p2.quote_hash_close_shape() {
                    if let Some(q1) = p1.quote_hash_open_shape().filter(|q1| *q1 >= q2) {
                        if q1 == q2 {
                            self.section = String::new();
                            self.factor = cfg.code_weight;
                        }
                    } else {
                        let rec = p2.reverse();
                        if !self.run.iter().any(|run| run.record == rec) {
                            self.run.push(Run::new(rec));
                        }
                    }
                } else if p2.quote_hash_open_shape().is_some() {
                    self.section = part.to_string();
                    self.factor = cfg.string_weight;
                } else {
                    self.count("expected:rawstring_content")
                }
            }
            _ => {}
        }
        self.factor
    }
    fn handle_first_part(&mut self, part: &str, cfg: &LanguageWeights) -> f64 {
        if part == "\n" {
            return self.handle_part_inner(part, cfg);
        }
        let do_buffer = match self.section.as_str() {
            "" => MARKERS
                .iter()
                .any(|(o, c)| o.starts_with(part) || (!c.starts_with('"') && c.starts_with(part))),
            section => section != "\"" && "\"*#/".contains(part),
        };
        if do_buffer {
            self.buffer.push_str(part);
            return 0.0;
        }
        self.handle_part_inner(part, cfg)
    }

    fn handle_part(&mut self, part: &str, cfg: &LanguageWeights) -> f64 {
        debug_assert!(!part.is_empty());
        debug_assert!(part.is_ascii());
        match (self.buffer.as_str(), part) {
            ("", part) => self.handle_first_part(part, cfg),
            // complete ones
            ("#", "\"") | ("*", "/") => {
                let op = std::mem::take(&mut self.buffer) + part;
                self.handle_part_inner(op.as_str(), cfg)
            }
            // extendable
            ("\"" | "#", "#") | ("/", "/" | "*") => {
                self.buffer.push_str(part);
                0.0
            }
            // more extendable
            (b, p) if b.starts_with(p.get_extendable()) => {
                self.buffer.push_str(part);
                0.0
            }
            // no combination
            _ => {
                let op = std::mem::take(&mut self.buffer);
                self.handle_part_inner(op.as_str(), cfg) + self.handle_first_part(part, cfg)
            }
        }
    }
}

impl PatchRefiner {
    pub fn evaluate(req: RefinementRequest) -> Result<RefinementResponse> {
        if let Some(sv) = &req.schema_version {
            ensure!(sv == SCHEMA_VERSION, "Unsupported schema_version '{sv}': expected '{SCHEMA_VERSION}'");
        }
        let config = req.config.clone().unwrap_or_default();
        config
            .semantic_checks
            .validate()
            .map_err(|e| anyhow!("Semantic checks config validation failed: {}", e))?;
        config.whitespace.validate().map_err(|e| {
            anyhow!("Whitespace config validation failed: {}", e)
        })?;
        let lang_weights = config.language_weights.clone().unwrap_or_default();
        lang_weights.validate().map_err(|e| anyhow!("Language weights validation failed: {e}"))?;
        let mode = Self::resolve_mode(&req);

        let perfect_patches = req.perfect_patches.clone().unwrap_or_default();

        Ok(if mode == ApplicationMode::Mode3 {
            Self::evaluate_mode_3(&req.original_code, &req.candidates, &config)
        } else {
            Self::evaluate_modes_1_2_4(
                &req.original_code,
                &req.candidates,
                &perfect_patches,
                mode,
                &config,
                &lang_weights,
            )
        })
    }

    fn resolve_mode(req: &RefinementRequest) -> ApplicationMode {
        if let Some(cfg) = &req.config
            && let Some(override_mode) = &cfg.mode_override
        {
            return override_mode.clone();
        }
        let perfects = req.perfect_patches.as_deref().unwrap_or(&[]);
        match perfects.len() {
            0 => ApplicationMode::Mode3,
            1 => {
                if perfects[0].reason.is_some() {
                    ApplicationMode::Mode1
                } else {
                    ApplicationMode::Mode2
                }
            }
            _ => ApplicationMode::Mode4,
        }
    }

    fn normalize_text(s: &str, cfg: &WhitespaceConfig) -> String {
        let mut text = s.to_string();
        if cfg.normalize_line_endings {
            text = text.replace("\r\n", "\n").replace('\r', "\n");
        }
        if cfg.ignore_whitespace {
            text = text
                .lines()
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join("\n");
        }
        text
    }

    fn compute_distance(changeset: &InlineChangeset, cfg: &LanguageWeights) -> f64 {
        let mut change = [ChangeSet::new(), ChangeSet::new()];
        let mut dist: f64;
        let mut compared = HashMap::new();

        loop {
            let mut key = None;
            'search: for i in 0..change[0].run.len() {
                for j in 0..change[1].run.len() {
                    if !compared.contains_key(&(i, j)) {
                        key = Some((i, j));
                        break 'search;
                    }
                }
            }
            let Some((i, j)) = key else { break };
            change[0].reset(cfg, i);
            change[1].reset(cfg, j);
            dist = 0.0;

            for op in changeset.diff().iter() {
                use prettydiff::basic::DiffOp::*;
                match op {
                    Remove(parts) => {
                        for part in parts.iter() {
                            dist += change[0].handle_part(part, cfg);
                        }
                    }
                    Insert(parts) => {
                        for part in parts.iter() {
                            dist += change[1].handle_part(part, cfg);
                        }
                    }
                    Replace(parts1, parts2) => {
                        for part in parts1.iter() {
                            dist += change[0].handle_part(part, cfg);
                        }
                        for part in parts2.iter() {
                            dist += change[1].handle_part(part, cfg);
                        }
                    }
                    Equal(parts) => {
                        for part in parts.iter() {
                            _ = change[0].handle_part(part, cfg);
                            _ = change[1].handle_part(part, cfg);
                        }
                    }
                }
            }
            dist += change.iter_mut().map(|c| c.finalize(cfg)).sum::<f64>();
            let key = (change[0].index, change[1].index);
            compared.insert(key, dist);
        }
        let mut best: Option<(bool, f64)> = None;
        let mut best_str = "No best".to_string();
        dist = f64::NAN;

        for ((i, j), v) in compared.iter() {
            change[0].index = *i;
            change[1].index = *j;
            let mut merged = change[0].counters().clone();
            for (k, n) in change[1].counters() {
                *merged.entry(k.clone()).or_insert(0) += n;
            }
            let (expected, unexpected) = evaluate(&merged);
            let closed_cleanly = change[0].section.is_empty() && change[1].section.is_empty();
            let score = hypothesis_score(closed_cleanly, expected, unexpected);

            if best.is_none_or(|b| score > b) {
                best = Some(score);
                dist = *v;
                let token1 = change[0].run[*i].record.clone();
                let token2 = change[1].run[*j].record.clone();
                best_str = format!(
                    "[patch:{i}] '{token1}' clean={} [patch:{j}] '{token2}' clean={} (exp={expected}, unexp={unexpected})",
                    change[0].section.is_empty(),
                    change[1].section.is_empty()
                );
                if 3 * unexpected > unexpected + expected {
                    best_str += &format!("\n{}", changeset.format());
                }
            }
        }
        println!("{} {dist}", best_str);
        dist
    }

    fn evaluate_modes_1_2_4(
        original: &str,
        candidates: &[PatchCandidate],
        perfects: &[PerfectPatch],
        mode: ApplicationMode,
        config: &RefinementConfig,
        lang_weights: &LanguageWeights,
    ) -> RefinementResponse {
        let mut best_deviation: Option<Deviation> = None;
        let mut diagnostics = Vec::new();

        for candidate in candidates {
            let ai_patch = match Patch::from_str(&candidate.diff_content) {
                Ok(p) => p,
                Err(e) => {
                    diagnostics.push(Diagnostic {
                        level: DiagnosticLevel::Error,
                        category: DiagnosticCategory::PatchParse,
                        message: format!("Candidate {} invalid: {}", candidate.id, e),
                        location: None,
                    });
                    continue;
                }
            };

            let ai_result = match apply(original, &ai_patch) {
                Ok(res) => Self::normalize_text(&res, &config.whitespace),
                Err(e) => {
                    diagnostics.push(Diagnostic {
                        level: DiagnosticLevel::Error,
                        category: DiagnosticCategory::PatchApply,
                        message: format!("Candidate {} apply failed: {}", candidate.id, e),
                        location: None,
                    });
                    continue;
                }
            };

            for perfect in perfects {
                let p_patch = match Patch::from_str(&perfect.diff_content) {
                    Ok(p) => p,
                    Err(e) => {
                        diagnostics.push(Diagnostic {
                            level: DiagnosticLevel::Warning,
                            category: DiagnosticCategory::PatchParse,
                            message: format!("Perfect patch {} invalid: {}", perfect.id, e),
                            location: None,
                        });
                        continue;
                    }
                };

                let p_result = match apply(original, &p_patch) {
                    Ok(res) => Self::normalize_text(&res, &config.whitespace),
                    Err(e) => {
                        diagnostics.push(Diagnostic {
                            level: DiagnosticLevel::Warning,
                            category: DiagnosticCategory::PatchApply,
                            message: format!("Perfect patch {} apply failed: {}", perfect.id, e),
                            location: None,
                        });
                        continue;
                    }
                };

                if ai_result == p_result {
                    return RefinementResponse {
                        schema_version: crate::models::SCHEMA_VERSION.to_string(),
                        mode,
                        decision: Decision::Approved,
                        selected_patch_id: Some(candidate.id.clone()),
                        matched_perfect_patch_id: Some(perfect.id.clone()),
                        deviations: None,
                        reasoning: perfect.reason.clone(),
                        diagnostics: vec![],
                    };
                }
                let changeset = prettydiff::diff_words(&ai_result, &p_result);
                let deviation_str = changeset.format();
                let distance = Self::compute_distance(&changeset, &lang_weights);

                if best_deviation
                    .as_ref()
                    .is_none_or(|d| distance < d.distance_score)
                {
                    best_deviation = Some(Deviation {
                        candidate_id: candidate.id.clone(),
                        closest_perfect_patch_id: perfect.id.clone(),
                        diff_from_perfect: deviation_str,
                        distance_score: distance,
                    });
                }
            }
        }

        let closest_reasoning = best_deviation.as_ref().and_then(|d| {
            perfects
                .iter()
                .find(|p| p.id == d.closest_perfect_patch_id)
                .and_then(|p| p.reason.clone())
        });

        RefinementResponse {
            schema_version: crate::models::SCHEMA_VERSION.to_string(),
            mode,
            decision: Decision::Rejected,
            selected_patch_id: None,
            matched_perfect_patch_id: None,
            deviations: best_deviation,
            reasoning: closest_reasoning,
            diagnostics,
        }
    }

    fn evaluate_mode_3(
        original: &str,
        candidates: &[PatchCandidate],
        config: &RefinementConfig,
    ) -> RefinementResponse {
        let mut diagnostics = Vec::new();
        let checkers: Vec<Box<dyn SemanticChecker>> =
            vec![Box::new(CompileChecker), Box::new(TestChecker)];

        for candidate in candidates {
            if let Ok(patch) = Patch::from_str(&candidate.diff_content)
                && let Ok(ai_result) = apply(original, &patch)
            {
                let mut all_ok = true;

                for checker in &checkers {
                    let diags = checker.check(original, &ai_result, &config.semantic_checks);
                    if diags.iter().any(|d| d.level == DiagnosticLevel::Error) {
                        all_ok = false;
                    }
                    diagnostics.extend(diags);
                }

                if all_ok {
                    return RefinementResponse {
                        schema_version: crate::models::SCHEMA_VERSION.to_string(),
                        mode: ApplicationMode::Mode3,
                        decision: Decision::Approved,
                        selected_patch_id: Some(candidate.id.clone()),
                        matched_perfect_patch_id: None,
                        deviations: None,
                        reasoning: None,
                        diagnostics,
                    };
                }
            }
        }

        RefinementResponse {
            schema_version: crate::models::SCHEMA_VERSION.to_string(),
            mode: ApplicationMode::Mode3,
            decision: Decision::Failed,
            selected_patch_id: None,
            matched_perfect_patch_id: None,
            deviations: None,
            reasoning: None,
            diagnostics,
        }
    }
}
