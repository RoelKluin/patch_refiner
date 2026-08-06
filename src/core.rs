use crate::checkers::{CompileChecker, SemanticChecker};
use crate::models::*;
use anyhow::{anyhow, Result};
use diffy::{apply, create_patch, Patch};

pub struct PatchRefiner;

impl PatchRefiner {
    pub fn evaluate(req: RefinementRequest) -> Result<RefinementResponse> {
        let config = req.config.clone().unwrap_or_default();
        config
            .semantic_checks
            .validate()
            .map_err(|e| anyhow!("Semantic checks config validation failed: {}", e))?;
        config
            .similarity
            .validate()
            .map_err(|e| anyhow!("Similarity config validation failed: {}", e))?;
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
            )
        })
    }

    fn resolve_mode(req: &RefinementRequest) -> ApplicationMode {
        if let Some(cfg) = &req.config {
            if let Some(override_mode) = &cfg.mode_override {
                return override_mode.clone();
            }
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

    fn compute_distance<T: ToOwned + ?Sized>(patch: Patch<T>, cfg: &SimilarityConfig) -> usize {
        let mut dist = 0.0;
        for hunk in patch.hunks() {
            for line in hunk.lines() {
                match line {
                    diffy::Line::Insert(_) => dist += cfg.add_weight,
                    diffy::Line::Delete(_) => dist += cfg.del_weight,
                    diffy::Line::Context(_) => {}
                }
            }
        }
        dist.round() as usize
    }

    fn evaluate_modes_1_2_4(
        original: &str,
        candidates: &[PatchCandidate],
        perfects: &[PerfectPatch],
        mode: ApplicationMode,
        config: &RefinementConfig,
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

                let deviation_diff = create_patch(&ai_result, &p_result);
                let deviation_str = deviation_diff.to_string();
                let distance = Self::compute_distance(deviation_diff, &config.similarity);

                if best_deviation
                    .as_ref()
                    .map_or(true, |d| distance < d.distance_score)
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
        let checkers: Vec<Box<dyn SemanticChecker>> = vec![Box::new(CompileChecker)];

        for candidate in candidates {
            if let Ok(patch) = Patch::from_str(&candidate.diff_content) {
                if let Ok(ai_result) = apply(original, &patch) {
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
