use crate::split::types::NamingConfig;
use crate::naming::{NamingValidator, NamingValidation, TemplateContext};
use crate::naming::resolver;

#[tauri::command]
pub fn validate_naming_template(
    template: &str,
    config: NamingConfig,
    contexts: Vec<TemplateContext>,
    total_count: Option<usize>,
) -> NamingValidation {
    NamingValidator::validate(template, &config, &contexts, total_count)
}

#[tauri::command]
pub fn resolve_naming(
    template: &str,
    config: NamingConfig,
    context: TemplateContext,
) -> String {
    resolver::resolve_template(template, &config, &context)
}

#[tauri::command]
pub fn preview_naming_batch(
    template: &str,
    config: NamingConfig,
    contexts: Vec<TemplateContext>,
) -> Vec<String> {
    resolver::resolve_batch(template, &config, &contexts)
}

#[tauri::command]
pub fn write_text_file(path: &str, content: &str) -> Result<(), String> {
    let path_obj = std::path::Path::new(path);
    let stem = path_obj.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if !stem.ends_with("_preview") {
        return Err("Only files with _preview suffix are allowed".to_string());
    }
    let final_path = if path_obj.is_absolute() || path.contains("..") {
        return Err("Invalid path: absolute paths and .. traversal are not allowed".to_string());
    } else {
        path
    };
    std::fs::write(final_path, content).map_err(|e| format!("Failed to write file: {}", e))
}