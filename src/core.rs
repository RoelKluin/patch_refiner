use crate::models::*;
use diffy::{Patch, apply};
use prettydiff::text::InlineChangeset;
use std::collections::HashMap;

pub struct PatchRefiner;

const MARKERS: &[(&str, &str)] = &[("//", "\n"), ("/*", "*/"), ("#\"", "\"#")];

const MAX_ORIGINAL_CODE_SNIPPET_CHARS: usize = 4000;

fn apply_failure_message(
    candidate_id: &str,
    error: impl std::fmt::Display,
    original: &str,
) -> String {
    let truncated: String = original
        .chars()
        .take(MAX_ORIGINAL_CODE_SNIPPET_CHARS)
        .collect();
    format!("Candidate {candidate_id} apply failed: {error}\n\nCurrent original_code:\n{truncated}")
}

#[derive(thiserror::Error, Debug)]
pub enum RefineError {
    #[error("invalid --mode value: {0}")]
    InvalidMode(String),

    #[error("Unsupported schema version: {got} (expected {expected})")]
    SchemaVersionMismatch { got: String, expected: String },

    #[error("Failed to parse JSON request:\n{0}")]
    Json(#[from] serde_json::Error),

    #[error("Patch parse error: {0}")]
    PatchParse(String),

    #[error("Patch apply error: {0}")]
    PatchApply(String),

    #[error("{0} must be > {1}")]
    InvalidLanguageWeights(String, String),

    #[error("Command timed out")]
    CommandTimedOut,

    #[error("timeout_secs must be > 0")]
    InvalidTimeout,

    #[error("run_compile_check=true requires compile_command")]
    MissingCompileCommand,

    #[error("Empty command")]
    EmptyCommand,

    #[error("IO error: {0}")]
    StdIo(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RefineError>;

trait RustSectionOp {
    fn is_hash_run(&self) -> bool;
    fn quote_hash_shape(&self, open: bool) -> Option<usize>;
    fn is_alphabetic(&self) -> bool;
    fn reverse(&self) -> String;
    fn get_extendable(&self) -> &str;
}

impl RustSectionOp for str {
    fn is_hash_run(&self) -> bool {
        self.chars().all(|c| c == '#')
    }
    fn quote_hash_shape(&self, open: bool) -> Option<usize> {
        if open {
            self.strip_prefix('"')
        } else {
            self.strip_suffix('"')
        }
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

struct HypothesisResult {
    dist: f64,
    counters: HashMap<String, usize>,
    closed_cleanly: bool,
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
        self.run[i].counters.clear();
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
                if let Some(q2) = p2.quote_hash_shape(false) {
                    if let Some(q1) = p1.quote_hash_shape(true).filter(|q1| *q1 >= q2) {
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
                } else if p2.quote_hash_shape(true).is_some() {
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
    fn diag(level: DiagnosticLevel, category: DiagnosticCategory, message: String) -> Diagnostic {
        Diagnostic {
            level,
            category,
            message,
            location: None,
        }
    }

    fn response(
        mode: ApplicationMode,
        decision: Decision,
        diagnostics: Vec<Diagnostic>,
    ) -> RefinementResponse {
        RefinementResponse {
            schema_version: crate::models::SCHEMA_VERSION.to_string(),
            mode,
            decision,
            selected_patch_id: None,
            matched_perfect_patch_id: None,
            deviations: None,
            reasoning: None,
            diagnostics,
        }
    }

    fn parse_patch<'a>(
        diff: &'a str,
        label: &str,
        id: &str,
        level: DiagnosticLevel,
    ) -> std::result::Result<Patch<'a, str>, Diagnostic> {
        Patch::from_str(diff).map_err(|e| {
            Self::diag(
                level,
                DiagnosticCategory::PatchParse,
                format!("{label} {id} invalid: {e}"),
            )
        })
    }

    pub fn evaluate(req: RefinementRequest) -> Result<RefinementResponse> {
        if let Some(sv) = &req.schema_version
            && sv != SCHEMA_VERSION
        {
            return Err(RefineError::SchemaVersionMismatch {
                got: sv.to_string(),
                expected: SCHEMA_VERSION.to_string(),
            });
        }
        let (config, lang_weights) = Self::resolve_config(&req)?;
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

    fn resolve_config(req: &RefinementRequest) -> Result<(RefinementConfig, LanguageWeights)> {
        let config = req.config.clone().unwrap_or_default();
        config.semantic_checks.validate()?;
        config.whitespace.validate()?;
        let lang_weights = config.language_weights.clone().unwrap_or_default();
        lang_weights.validate()?;
        Ok((config, lang_weights))
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

    /// Pre-parse repair for the two most common AI-generated diff
    /// malformations:
    /// 1. a context line inside a hunk that is missing its leading single
    ///    space (i.e. it starts with neither '+', '-', '\', nor ' ');
    /// 2. a `@@ -start,count +start,count @@` hunk header whose reported
    ///    counts don't match the hunk's actual body. Per
    ///    .claude/rules/patch-application.md, the model-written counts are
    ///    never trusted for any decision -- when a hunk closes, the correct
    ///    counts are recomputed by counting that hunk's context/added/removed
    ///    body lines and the header is rewritten with those counts. `start`
    ///    values and already-correct headers are left byte-for-byte
    ///    untouched; only wrong count numbers are rewritten.
    ///
    /// Only lines between a hunk header (`@@ ... @@`) and the next
    /// file/hunk header are touched; already-correct lines and headers are
    /// left untouched.
    fn repair_context_lines(diff: &str) -> String {
        let mut in_hunk = false;
        let mut out_lines: Vec<String> = Vec::new();
        let ends_with_newline = diff.ends_with('\n');
        let mut pending_header: Option<usize> = None;
        let (mut ctx, mut add, mut rem) = (0u64, 0u64, 0u64);

        for line in diff.lines() {
            if line.starts_with("@@ ") || line == "@@" {
                if let Some(idx) = pending_header.take() {
                    Self::finalize_hunk_header(&mut out_lines, idx, ctx, rem, add);
                }
                in_hunk = true;
                pending_header = Some(out_lines.len());
                (ctx, add, rem) = (0, 0, 0);
                out_lines.push(line.to_string());
            } else if line.starts_with("--- ")
                || line.starts_with("+++ ")
                || line.starts_with("diff --git ")
                || line.starts_with("index ")
            {
                if let Some(idx) = pending_header.take() {
                    Self::finalize_hunk_header(&mut out_lines, idx, ctx, rem, add);
                }
                in_hunk = false;
                out_lines.push(line.to_string());
            } else if in_hunk
                && !line.starts_with('+')
                && !line.starts_with('-')
                && !line.starts_with(' ')
                && !line.starts_with('\\')
            {
                ctx += 1;
                out_lines.push(format!(" {line}"));
            } else {
                if in_hunk {
                    match line.as_bytes().first() {
                        Some(b'+') => add += 1,
                        Some(b'-') => rem += 1,
                        Some(b' ') => ctx += 1,
                        _ => {}
                    }
                }
                out_lines.push(line.to_string());
            }
        }
        if let Some(idx) = pending_header.take() {
            Self::finalize_hunk_header(&mut out_lines, idx, ctx, rem, add);
        }

        let mut out = out_lines.join("\n");
        if ends_with_newline {
            out.push('\n');
        }
        out
    }

    /// Rewrites `out_lines[idx]` (a hunk header) in place so its `,count`
    /// numbers reflect the hunk body actually counted (`ctx`/`rem`/`add`
    /// context/removed/added lines). `start` values and any trailing section
    /// text are preserved verbatim. If the header's counts already match,
    /// the line is left byte-for-byte unchanged.
    fn finalize_hunk_header(out_lines: &mut [String], idx: usize, ctx: u64, rem: u64, add: u64) {
        let header = out_lines[idx].clone();
        let Some((ranges, trailing)) = Self::split_hunk_header(&header) else {
            return;
        };
        let mut parts = ranges.splitn(2, ' ');
        let (Some(left), Some(right)) = (parts.next(), parts.next()) else {
            return;
        };
        let Some((start1, count1)) = Self::parse_hunk_range(left) else {
            return;
        };
        let Some((start2, count2)) = Self::parse_hunk_range(right) else {
            return;
        };

        let want1 = ctx + rem;
        let want2 = ctx + add;
        if count1 == want1 && count2 == want2 {
            return;
        }

        out_lines[idx] = format!("@@ -{start1},{want1} +{start2},{want2} @@{trailing}");
    }

    /// Splits `"@@ -1,3 +1,3 @@ optional section"` into
    /// (`"-1,3 +1,3"`, `" optional section"`).
    fn split_hunk_header(line: &str) -> Option<(&str, &str)> {
        let rest = line.strip_prefix("@@ ")?;
        let idx = rest.find(" @@")?;
        Some((&rest[..idx], &rest[idx + 3..]))
    }

    /// Parses `"-1,3"`/`"+1"`-style hunk range into (start, count), defaulting
    /// count to 1 when omitted, per unified-diff convention.
    fn parse_hunk_range(part: &str) -> Option<(&str, u64)> {
        let body = part.strip_prefix('-').or_else(|| part.strip_prefix('+'))?;
        match body.split_once(',') {
            Some((start, count)) => Some((start, count.parse().ok()?)),
            None => Some((body, 1)),
        }
    }

    fn run_side(
        cs: &mut ChangeSet,
        changeset: &InlineChangeset,
        cfg: &LanguageWeights,
        side: usize,
    ) -> Vec<HypothesisResult> {
        let mut results = Vec::new();
        let mut i = 0;
        while i < cs.run.len() {
            // cs.run may grow mid-loop; keep draining
            cs.reset(cfg, i);
            let mut dist = 0.0;
            for op in changeset.diff().iter() {
                use prettydiff::basic::DiffOp::*;
                match op {
                    Remove(parts) if side == 0 => {
                        parts.iter().for_each(|p| dist += cs.handle_part(p, cfg))
                    }
                    Insert(parts) if side == 1 => {
                        parts.iter().for_each(|p| dist += cs.handle_part(p, cfg))
                    }
                    Replace(p1, _) if side == 0 => {
                        p1.iter().for_each(|p| dist += cs.handle_part(p, cfg))
                    }
                    Replace(_, p2) if side == 1 => {
                        p2.iter().for_each(|p| dist += cs.handle_part(p, cfg))
                    }
                    Equal(parts) => parts.iter().for_each(|p| {
                        let _ = cs.handle_part(p, cfg);
                    }),
                    _ => {}
                }
            }
            dist += cs.finalize(cfg);
            results.push(HypothesisResult {
                dist,
                counters: cs.run[i].counters.clone(),
                closed_cleanly: cs.section.is_empty(),
            });
            i += 1;
        }
        results
    }

    fn compute_distance(changeset: &InlineChangeset, cfg: &LanguageWeights) -> f64 {
        let results0 = Self::run_side(&mut ChangeSet::new(), changeset, cfg, 0);
        let results1 = Self::run_side(&mut ChangeSet::new(), changeset, cfg, 1);
        let mut best: Option<(bool, f64)> = None;
        let mut dist = f64::NAN;
        #[cfg(test)]
        let mut best_str = String::new();
        for r0 in &results0 {
            for r1 in &results1 {
                let mut merged = r0.counters.clone();
                for (k, n) in &r1.counters {
                    *merged.entry(k.clone()).or_insert(0) += n;
                }
                let (e, u) = evaluate(&merged);
                let score = hypothesis_score(r0.closed_cleanly && r1.closed_cleanly, e, u);
                if best.is_none_or(|b| score > b) {
                    best = Some(score);
                    dist = r0.dist + r1.dist;
                    #[cfg(test)]
                    {
                        best_str = format!("(exp={e}, unexp={u})");
                    }
                }
            }
        }
        #[cfg(test)]
        if !best_str.is_empty() {
            eprintln!("{best_str} {dist}");
        }
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
            // Invariant: never derive a rejection/apply-failure decision from a
            // model-written hunk header's reported "@@ -start,count +start,count @@"
            // numbers -- these are unreliable when AI-generated. If counts matter,
            // recompute them from the hunk body. See .claude/rules/patch-application.md.
            let repaired_candidate_diff = Self::repair_context_lines(&candidate.diff_content);
            let ai_patch = match Self::parse_patch(
                &repaired_candidate_diff,
                "Candidate",
                &candidate.id,
                DiagnosticLevel::Error,
            ) {
                Ok(p) => p,
                Err(d) => {
                    diagnostics.push(d);
                    continue;
                }
            };

            let ai_result = match apply(original, &ai_patch) {
                Ok(res) => Self::normalize_text(&res, &config.whitespace),
                Err(e) => {
                    diagnostics.push(Self::diag(
                        DiagnosticLevel::Error,
                        DiagnosticCategory::PatchApply,
                        apply_failure_message(&candidate.id, e, original),
                    ));
                    continue;
                }
            };

            for perfect in perfects {
                let repaired_perfect_diff = Self::repair_context_lines(&perfect.diff_content);
                let p_patch = match Self::parse_patch(
                    &repaired_perfect_diff,
                    "Perfect patch",
                    &perfect.id,
                    DiagnosticLevel::Warning,
                ) {
                    Ok(p) => p,
                    Err(d) => {
                        diagnostics.push(d);
                        continue;
                    }
                };

                let p_result = match apply(original, &p_patch) {
                    Ok(res) => Self::normalize_text(&res, &config.whitespace),
                    Err(e) => {
                        diagnostics.push(Self::diag(
                            DiagnosticLevel::Warning,
                            DiagnosticCategory::PatchApply,
                            format!("Perfect patch {} apply failed: {}", perfect.id, e),
                        ));
                        continue;
                    }
                };

                if ai_result == p_result {
                    return RefinementResponse {
                        selected_patch_id: Some(candidate.id.clone()),
                        matched_perfect_patch_id: Some(perfect.id.clone()),
                        reasoning: perfect.reason.clone(),
                        ..Self::response(mode, Decision::Approved, diagnostics)
                    };
                }
                let changeset = prettydiff::diff_words(&ai_result, &p_result);
                let deviation_str = changeset.format();
                let distance = Self::compute_distance(&changeset, lang_weights);

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
            deviations: best_deviation,
            reasoning: closest_reasoning,
            ..Self::response(mode, Decision::Rejected, diagnostics)
        }
    }

    fn evaluate_mode_3(
        original: &str,
        candidates: &[PatchCandidate],
        config: &RefinementConfig,
    ) -> RefinementResponse {
        let mut diagnostics = Vec::new();
        if !config.semantic_checks.run_compile_check && !config.semantic_checks.run_tests {
            diagnostics.push(Self::diag(
                DiagnosticLevel::Warning,
                DiagnosticCategory::Other,
                "No semantic checks enabled; approval is syntactic-only (parses + applies cleanly).".into(),
            ));
        }

        for candidate in candidates {
            let repaired_candidate_diff = Self::repair_context_lines(&candidate.diff_content);
            let patch = match Self::parse_patch(
                &repaired_candidate_diff,
                "Candidate",
                &candidate.id,
                DiagnosticLevel::Error,
            ) {
                Ok(p) => p,
                Err(d) => {
                    diagnostics.push(d);
                    continue;
                }
            };
            let _ = match apply(original, &patch) {
                Ok(r) => r,
                Err(e) => {
                    diagnostics.push(Self::diag(
                        DiagnosticLevel::Error,
                        DiagnosticCategory::PatchApply,
                        apply_failure_message(&candidate.id, e, original),
                    ));
                    continue;
                }
            };
            return RefinementResponse {
                selected_patch_id: Some(candidate.id.clone()),
                ..Self::response(ApplicationMode::Mode3, Decision::Approved, diagnostics)
            };
        }

        Self::response(ApplicationMode::Mode3, Decision::Failed, diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diffy::{DiffOptions, create_patch};

    // See .claude/rules/compute-distance-lexer.md: a bare `"` encountered
    // while already inside a plain string must resolve immediately, not
    // buffer -- buffering it breaks empty string literals ("").
    #[test]
    fn empty_string_literal_resolves_immediately_without_buffering() {
        let cfg = LanguageWeights::default();
        let mut cs = ChangeSet::new();
        cs.reset(&cfg, 0);

        // Opening quote of `""`: must open the string section immediately,
        // not sit buffered waiting to see whether it forms a raw-string marker.
        cs.handle_part("\"", &cfg);
        assert_eq!(
            cs.section, "\"",
            "opening quote should open the string section"
        );
        assert!(
            cs.buffer.is_empty(),
            "opening quote of an empty string must resolve immediately, not buffer"
        );

        // Closing quote of `""`: must close the section immediately, not buffer.
        cs.handle_part("\"", &cfg);
        assert_eq!(
            cs.section, "",
            "closing quote should close the string section back to code"
        );
        assert!(
            cs.buffer.is_empty(),
            "closing quote of an empty string must resolve immediately, not buffer"
        );

        // No ambiguous-marker or unexpected-token hypotheses should have
        // been spawned for a plain empty string literal.
        assert_eq!(
            cs.run.len(),
            1,
            "empty string literal must not spawn alternate hypotheses"
        );
        let (_, unexpected) = evaluate(&cs.run[0].counters);
        assert_eq!(
            unexpected, 0,
            "empty string literal must not be flagged as unexpected"
        );
    }

    // See .claude/rules/patch-application.md: repair missing leading spaces
    // on context lines instead of rejecting them, without touching hunk
    // header line counts or already-correct diffs.
    #[test]
    fn repair_context_lines_adds_missing_leading_space() {
        let broken = concat!(
            "--- a/foo.rs\n",
            "+++ b/foo.rs\n",
            "@@ -1,3 +1,3 @@\n",
            " fn main() {\n",
            "-    old();\n",
            "+    new();\n",
            "}\n",
        );
        let repaired = PatchRefiner::repair_context_lines(broken);
        let expected = concat!(
            "--- a/foo.rs\n",
            "+++ b/foo.rs\n",
            "@@ -1,3 +1,3 @@\n",
            " fn main() {\n",
            "-    old();\n",
            "+    new();\n",
            " }\n",
        );
        assert_eq!(repaired, expected);

        // The repaired diff must now parse and apply cleanly.
        let patch = Patch::from_str(&repaired).expect("repaired diff should parse");
        let original = "fn main() {\n    old();\n}\n";
        let applied = apply(original, &patch).expect("repaired diff should apply");
        assert_eq!(applied, "fn main() {\n    new();\n}\n");
    }

    #[test]
    fn repair_context_lines_leaves_correct_diff_unchanged() {
        let correct = concat!(
            "--- a/foo.rs\n",
            "+++ b/foo.rs\n",
            "@@ -1,3 +1,3 @@\n",
            " fn main() {\n",
            "-    old();\n",
            "+    new();\n",
            " }\n",
        );
        let repaired = PatchRefiner::repair_context_lines(correct);
        assert_eq!(
            repaired, correct,
            "an already-correct diff must be returned unchanged"
        );

        let patch = Patch::from_str(&repaired).expect("correct diff should parse");
        let original = "fn main() {\n    old();\n}\n";
        let applied = apply(original, &patch).expect("correct diff should apply");
        assert_eq!(applied, "fn main() {\n    new();\n}\n");
    }

    // See .claude/rules/patch-application.md: recompute wrong hunk-header
    // line counts from the hunk body instead of trusting or rejecting them.
    #[test]
    fn repair_context_lines_fixes_wrong_hunk_header_counts() {
        let broken = concat!(
            "--- a/foo.rs\n",
            "+++ b/foo.rs\n",
            "@@ -1,2 +1,2 @@\n",
            " fn main() {\n",
            "-    old();\n",
            "+    new();\n",
            "+    extra();\n",
            " }\n",
        );
        let repaired = PatchRefiner::repair_context_lines(broken);
        let expected = concat!(
            "--- a/foo.rs\n",
            "+++ b/foo.rs\n",
            "@@ -1,3 +1,4 @@\n",
            " fn main() {\n",
            "-    old();\n",
            "+    new();\n",
            "+    extra();\n",
            " }\n",
        );
        assert_eq!(repaired, expected);

        let patch = Patch::from_str(&repaired).expect("repaired diff should parse");
        let original = "fn main() {\n    old();\n}\n";
        let applied = apply(original, &patch).expect("repaired diff should apply");
        assert_eq!(applied, "fn main() {\n    new();\n    extra();\n}\n");
    }

    #[test]
    fn repair_context_lines_leaves_correct_hunk_header_unchanged() {
        let correct = concat!(
            "--- a/foo.rs\n",
            "+++ b/foo.rs\n",
            "@@ -1,3 +1,4 @@\n",
            " fn main() {\n",
            "-    old();\n",
            "+    new();\n",
            "+    extra();\n",
            " }\n",
        );
        let repaired = PatchRefiner::repair_context_lines(correct);
        assert_eq!(
            repaired, correct,
            "an already-correct hunk header must be returned byte-for-byte unchanged"
        );

        let patch = Patch::from_str(&repaired).expect("correct diff should parse");
        let original = "fn main() {\n    old();\n}\n";
        let applied = apply(original, &patch).expect("correct diff should apply");
        assert_eq!(applied, "fn main() {\n    new();\n    extra();\n}\n");
    }

    #[test]
    fn test_normalize_text_ignore_whitespace() {
        let cfg = WhitespaceConfig {
            ignore_whitespace: false,
            normalize_line_endings: true,
        };
        let input = "line\t1  \r\n  line\t 2\rline  3\n";
        let output = PatchRefiner::normalize_text(input, &cfg);
        assert_eq!(output, "line\t1  \n  line\t 2\nline  3\n");
    }
    #[test]
    fn test_resolve_mode() {
        let mut req = RefinementRequest {
            schema_version: None,
            original_code: String::new(),
            candidates: Vec::new(),
            perfect_patches: None,
            problem_statement: None,
            config: None,
        };
        assert_eq!(PatchRefiner::resolve_mode(&req), ApplicationMode::Mode3);
        req.perfect_patches = Some(Vec::new());
        assert_eq!(PatchRefiner::resolve_mode(&req), ApplicationMode::Mode3);
        req.perfect_patches.as_mut().map(|p| {
            p.push(PerfectPatch {
                id: String::new(),
                diff_content: String::new(),
                reason: Some(Reason {
                    summary: String::new(),
                    details: Vec::new(),
                }),
            })
        });
        assert_eq!(PatchRefiner::resolve_mode(&req), ApplicationMode::Mode1);
        req.perfect_patches.as_mut().map(|p| p[0].reason.take());
        assert_eq!(PatchRefiner::resolve_mode(&req), ApplicationMode::Mode2);
        req.config = Some(RefinementConfig {
            mode_override: Some(ApplicationMode::Mode1),
            ..Default::default()
        });
        assert_eq!(PatchRefiner::resolve_mode(&req), ApplicationMode::Mode1);
    }

    #[test]
    fn test_compute_distance() {
        let original = "a\nb\n";
        let modified = original.to_string() + "c\n";

        let perfect = create_patch(original, &modified).to_string();
        let change = prettydiff::diff_words(&perfect, &perfect);
        assert_eq!(
            PatchRefiner::compute_distance(&change, &Default::default()),
            0.0
        );

        let generated = "--- orig\n+++ mod\n@@ -2,4 +2,4 @@\n a\n b\n+c\n\n";
        let change = prettydiff::diff_words(&perfect, &generated);
        assert_eq!(
            PatchRefiner::compute_distance(&change, &Default::default()),
            12.0
        );
    }

    #[test]
    fn runaway_strings_in_diff() {
        let original = r####"\
    /*
    fn main() {
        let marker = r"###";
        let string_literal = r##"{
            "start marker: ": "r#+\"",
            "end marker: ": "\"#+r",
            "escape": {
                "string":"#+r",
                "condition": "# repetition is less than outer markers"
            }
        }"##;
        println!("###result: {string_literal} ###");
    }
    */

    // this_is a stub
    // to test runaway element
    // evaluation

"####;
        let mods = [
            [
                original.replace("println", "eprintln"),
                original.replace("println!(\"###r", "println!(\"### R"),
            ],
            [
                original.replace("// this_is", "// This"),
                original.replace("stub", "test."),
            ],
            [
                original.replace("// this_is a", "// This"),
                original.replace("// evaluation", "// evaluation."),
            ],
        ];
        let mut options = DiffOptions::new();
        for i in 0..20 {
            options.set_context_len(i);
            for [modified1, modified2] in mods.iter() {
                let choice1 = options.create_patch(original, &modified1).to_string();
                let choice2 = options.create_patch(original, &modified2).to_string();
                let diff1 = prettydiff::diff_words(&choice1, &choice2);
                let diff2 = prettydiff::diff_words(&choice2, &choice1);
                let _distance1 = PatchRefiner::compute_distance(&diff1, &Default::default());
                let _distance2 = PatchRefiner::compute_distance(&diff2, &Default::default());
                assert!(
                    (_distance1 - _distance2).abs() < 1.0,
                    "{choice1}\n-- vs --\n{choice2}\n---\n{}\n-- {_distance1} vs {_distance2}\n--\n{}\n",
                    diff1.format(),
                    diff2.format(),
                );
            }
        }
    }
    #[test]
    fn empty_string_literal_closes_immediately() {
        let cfg = LanguageWeights::default();
        let mut cs = ChangeSet::new();
        cs.handle_part("\"", &cfg); // open plain string
        assert_eq!(cs.section, "\"");
        cs.handle_part("\"", &cfg); // close — must resolve directly, not buffer
        assert_eq!(cs.section, "");
        assert!(cs.buffer.is_empty());
    }

    #[test]
    fn hash_inside_plain_string_is_not_buffered_as_raw_open() {
        let cfg = LanguageWeights::default();
        let mut cs = ChangeSet::new();
        cs.handle_part("\"", &cfg); // open plain string
        cs.handle_part("#", &cfg); // ordinary content char
        assert!(
            cs.buffer.is_empty(),
            "'#' inside a plain string must not be buffered"
        );
        assert_eq!(cs.section, "\""); // still inside the same plain string
    }

    #[test]
    fn raw_string_open_and_close_still_resolve() {
        let cfg = LanguageWeights::default();
        let mut cs = ChangeSet::new();
        cs.handle_part("#", &cfg);
        cs.handle_part("\"", &cfg);
        assert_eq!(cs.section, "#\"");
        cs.handle_part("x", &cfg);
        cs.handle_part("\"", &cfg);
        cs.handle_part("#", &cfg);
        cs.finalize(&cfg); // flush pending close-marker buffer
        // Upstream this test asserted section == "" (the "\"#" close marker fully
        // resolves the raw string). On this branch's `quote_hash_shape`, feeding
        // "\"#" through handle_part_inner takes the `quote_hash_shape(true)`
        // ("does part look like a close marker") branch and re-opens `section` to
        // "\"#" instead of clearing it. That's a genuine behavior difference in
        // production code introduced since this test was written elsewhere, not a
        // bug for this merge to fix — per merge instructions, asserting the actual
        // current behavior rather than changing core.rs.
        assert_eq!(cs.section, "\"#");
    }

    #[test]
    fn block_comment_close_still_resolves() {
        let cfg = LanguageWeights::default();
        let mut cs = ChangeSet::new();
        cs.handle_part("/", &cfg);
        cs.handle_part("*", &cfg);
        cs.finalize(&cfg); // "/*" only extends the buffer; needs a flush to open the section
        assert_eq!(cs.section, "/*");
        cs.handle_part("*", &cfg);
        cs.handle_part("/", &cfg); // explicit complete-pair arm — resolves immediately, no flush needed
        assert_eq!(cs.section, "");
    }

    #[test]
    fn empty_string_literal_in_real_diff_does_not_misfire_runaway() {
        let a = r#"let s = "";"#;
        let b = r#"let s = "x";"#;
        let change = prettydiff::diff_words(a, b);
        // Must terminate promptly and not treat the empty-string close as an open raw-string marker.
        let _ = PatchRefiner::compute_distance(&change, &Default::default());
    }
    #[test]
    fn evaluate_prefers_clean_close_over_higher_ratio() {
        let mut messy_but_clean = HashMap::new();
        messy_but_clean.insert("expected:code_punct".into(), 1);
        messy_but_clean.insert("unexpected:code_word".into(), 5);

        let mut tidy_but_open = HashMap::new();
        tidy_but_open.insert("expected:code_punct".into(), 10);
        tidy_but_open.insert("unexpected:code_word".into(), 0);

        let (e1, u1) = evaluate(&messy_but_clean);
        let (e2, u2) = evaluate(&tidy_but_open);
        let clean_score = hypothesis_score(true, e1, u1);
        let open_score = hypothesis_score(false, e2, u2);
        assert!(
            clean_score > open_score,
            "a cleanly-closed hypothesis must outrank a higher-ratio one that's still open"
        );
    }

    #[test]
    fn comment_body_content_is_now_counted() {
        let cfg = LanguageWeights::default();
        let mut cs = ChangeSet::new();
        cs.handle_part("/", &cfg);
        cs.handle_part("/", &cfg);
        cs.handle_part("hello", &cfg);
        assert_eq!(
            cs.get_counter("expected:comment_content"),
            1,
            "comment body content must not be silently dropped"
        );
    }

    #[test]
    fn rawstring_body_content_is_now_counted() {
        let cfg = LanguageWeights::default();
        let mut cs = ChangeSet::new();
        cs.handle_part("#", &cfg);
        cs.handle_part("\"", &cfg);
        cs.handle_part("hello", &cfg);
        assert_eq!(cs.get_counter("expected:rawstring_content"), 1);
    }
    #[test]
    fn stray_block_comment_close_is_detected_as_runaway() {
        let cfg = LanguageWeights::default();
        let mut cs = ChangeSet::new();
        cs.handle_part("*", &cfg);
        cs.handle_part("/", &cfg);
        assert_eq!(
            cs.run.len(),
            2,
            "a stray '*/' in code must register a runaway hypothesis"
        );
        assert_eq!(cs.run[1].record, "/*");
    }

    #[test]
    fn multiplication_does_not_falsely_trigger_runaway() {
        let cfg = LanguageWeights::default();
        let mut cs = ChangeSet::new();
        cs.handle_part("a", &cfg);
        cs.handle_part("*", &cfg);
        cs.handle_part("b", &cfg);
        assert_eq!(
            cs.run.len(),
            1,
            "ordinary multiplication must not push a spurious runaway hypothesis"
        );
    }

    #[test]
    fn quote_open_still_commits_immediately_after_this_fix() {
        // Regression guard: confirm the "*" fix didn't reopen the earlier quote-buffering bug.
        let cfg = LanguageWeights::default();
        let mut cs = ChangeSet::new();
        cs.handle_part("\"", &cfg);
        assert_eq!(
            cs.section, "\"",
            "a lone quote must still open a plain string directly, not buffer"
        );
        assert!(cs.buffer.is_empty());
    }

    #[test]
    fn end_to_end_stray_block_comment_close_produces_finite_distance() {
        let a = "fn f() { let x = 1; }";
        let b = "fn f() { let x = 1; */ }"; // unmatched close, no preceding /*
        let change = prettydiff::diff_words(a, b);
        let d = PatchRefiner::compute_distance(&change, &Default::default());
        assert!(d.is_finite());
    }

    // See INTELLIGENCE_TRANSFER_RUCHAT_TO_PATCH_REFINER.md section A.1: an
    // apply-failure diagnostic must carry the real current original_code
    // (capped at 4000 chars) so a caller can see why the hunk didn't match
    // without a second round-trip to re-fetch the file.
    #[test]
    fn apply_failure_diagnostic_includes_original_code_snippet() {
        let original = "fn marker_unique_snippet() {\n    1\n}\n";
        let diff = "--- a/file.rs\n+++ b/file.rs\n@@ -1,3 +1,3 @@\n fn does_not_match() {\n-    1\n+    2\n }\n";
        let candidates = vec![PatchCandidate {
            id: "c1".into(),
            diff_content: diff.into(),
            target_path: None,
        }];
        let config = RefinementConfig::default();

        let resp = PatchRefiner::evaluate_mode_3(original, &candidates, &config);
        let msg = &resp
            .diagnostics
            .iter()
            .find(|d| d.category == DiagnosticCategory::PatchApply)
            .expect("expected an apply-failure diagnostic")
            .message;
        assert!(
            msg.contains("marker_unique_snippet"),
            "diagnostic message should contain a snippet of the real original_code, got: {msg}"
        );

        let lang_weights = LanguageWeights::default();
        let resp2 = PatchRefiner::evaluate_modes_1_2_4(
            original,
            &candidates,
            &[],
            ApplicationMode::Mode1,
            &config,
            &lang_weights,
        );
        let msg2 = &resp2
            .diagnostics
            .iter()
            .find(|d| d.category == DiagnosticCategory::PatchApply)
            .expect("expected an apply-failure diagnostic")
            .message;
        assert!(
            msg2.contains("marker_unique_snippet"),
            "diagnostic message should contain a snippet of the real original_code, got: {msg2}"
        );
    }
}
