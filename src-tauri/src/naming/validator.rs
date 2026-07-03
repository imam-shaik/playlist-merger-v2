use serde::{Deserialize, Serialize};
use super::parser::{TemplateParser, Token, TokenKind};
use super::context::TemplateContext;
use crate::split::types::NamingConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NamingErrorCode {
    EmptyTemplate,
    NoVariables,
    UnknownVariable,
    UnclosedBrace,
    IllegalChars,
    ReservedName,
    PathTooLong,
    EmptyAfterSanitize,
    DuplicateOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingError {
    pub code: NamingErrorCode,
    pub message: String,
    pub position: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingValidation {
    pub valid: bool,
    pub errors: Vec<NamingError>,
    pub warnings: Vec<NamingWarning>,
    pub resolved_preview: Option<String>,
    pub batch_preview: Option<Vec<String>>,
    pub total_count: Option<usize>,
}

pub struct NamingValidator;

impl NamingValidator {
    const MAX_FILENAME_LEN: usize = 200;
    const RESERVED_NAMES: &'static [&'static str] = &[
        "CON", "PRN", "AUX", "NUL",
        "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    const ILLEGAL_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

    pub fn validate(
        template: &str,
        config: &NamingConfig,
        contexts: &[TemplateContext],
        total_count: Option<usize>,
    ) -> NamingValidation {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if template.trim().is_empty() {
            errors.push(NamingError {
                code: NamingErrorCode::EmptyTemplate,
                message: "Template cannot be empty".to_string(),
                position: None,
            });
            return NamingValidation {
                valid: false,
                errors,
                warnings,
                resolved_preview: None,
                batch_preview: None,
                total_count: None,
            };
        }

        let tokens = match TemplateParser::tokenize(template) {
            Ok(t) => t,
            Err(e) => {
                errors.push(e);
                return NamingValidation {
                    valid: false,
                    errors,
                    warnings,
                    resolved_preview: None,
                    batch_preview: None,
                    total_count: None,
                };
            }
        };

        let has_sequence_var = tokens.iter().any(|t| {
            matches!(t, Token(TokenKind::Variable(v)) if matches!(v.as_str(), "num" | "num2" | "num3" | "num4" | "playlist_index" | "original_num"))
        });

        if !has_sequence_var && contexts.len() > 1 {
            warnings.push(NamingWarning {
                code: "NO_SEQUENCE_VARIABLE".to_string(),
                message: "Template has no sequence variable — all outputs will have the same name".to_string(),
            });
        }

        let has_var_tokens = tokens.iter().any(|t| matches!(t, Token(TokenKind::Variable(_))));
        if !has_var_tokens && !template.is_empty() {
            warnings.push(NamingWarning {
                code: "NO_VARIABLES".to_string(),
                message: "Template contains no variables — output name will be static".to_string(),
            });
        }

        let first_ctx = contexts.first()                    .cloned().unwrap_or_else(|| TemplateContext {
            filename: "Video".to_string(),
            extension: "mp4".to_string(),
            folder: None,
            original_num: None,
            index: 1,
            start_time: 0.0,
            end_time: 30.0,
            duration: 30.0,
            chapter: Some("Introduction".to_string()),
            resolution: Some("1920x1080".to_string()),
            width: Some(1920),
            height: Some(1080),
            playlist: None,
            playlist_index: None,
            video_count: None,
            total_duration: None,
            date: "2026-01-01".to_string(),
            time: "00-00-00".to_string(),
            prefix: config.prefix.clone(),
            suffix: config.suffix.clone(),
            lang: None,
            lang_name: None,
            part_label: Some("Part".to_string()),
        });

        let resolved = super::resolver::resolve_template(template, config, &first_ctx);

        for c in Self::ILLEGAL_CHARS {
            if resolved.contains(c) {
                errors.push(NamingError {
                    code: NamingErrorCode::IllegalChars,
                    message: format!("Resolved filename contains illegal character '{}'", c),
                    position: None,
                });
                break;
            }
        }

        let stem = resolved.split('.').next().unwrap_or(&resolved).to_uppercase();
        if Self::RESERVED_NAMES.contains(&stem.as_str()) {
            errors.push(NamingError {
                code: NamingErrorCode::ReservedName,
                message: format!("'{}' is a reserved Windows filename", stem),
                position: None,
            });
        }

        if resolved.trim().is_empty() {
            errors.push(NamingError {
                code: NamingErrorCode::EmptyAfterSanitize,
                message: "Filename would be empty after sanitization".to_string(),
                position: None,
            });
        }

        if resolved.chars().count() > Self::MAX_FILENAME_LEN {
            errors.push(NamingError {
                code: NamingErrorCode::PathTooLong,
                message: format!(
                    "Filename would be {} characters (max: {})",
                    resolved.chars().count(),
                    Self::MAX_FILENAME_LEN
                ),
                position: None,
            });
        }

        let batch_preview = if !contexts.is_empty() {
            let preview_count = std::cmp::min(contexts.len(), 5);
            let preview_contexts = &contexts[..preview_count];
            Some(super::resolver::resolve_batch(template, config, preview_contexts))
        } else {
            None
        };

        if let (Some(ctxs), Some(total)) = (&batch_preview, total_count) {
            let mut seen = std::collections::HashSet::new();
            for name in ctxs.iter().cloned().chain(
                (ctxs.len()..total).map(|i| {
                    let mut dummy_ctx = first_ctx.clone();
                    dummy_ctx.index = i + 1;
                    super::resolver::resolve_template(template, config, &dummy_ctx)
                })
            ) {
                if !seen.insert(name.clone()) {
                    errors.insert(0, NamingError {
                        code: NamingErrorCode::DuplicateOutput,
                        message: format!("Duplicate output filename detected: '{}'", name),
                        position: None,
                    });
                    break;
                }
            }
        }

        let is_valid = errors.is_empty();
        NamingValidation {
            valid: is_valid,
            errors,
            warnings,
            resolved_preview: if is_valid { Some(resolved) } else { None },
            batch_preview,
            total_count,
        }
    }
}