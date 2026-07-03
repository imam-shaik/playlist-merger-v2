use super::context::TemplateContext;
use super::parser::{TemplateParser, Token, TokenKind};
use crate::split::types::NamingConfig;

pub fn resolve_template(template: &str, config: &NamingConfig, ctx: &TemplateContext) -> String {
    let tokens = match TemplateParser::tokenize(template) {
        Ok(t) => t,
        Err(_) => return template.to_string(),
    };

    let padding = config.zero_padding.unwrap_or(3) as usize;
    let mut result = String::new();

    for Token(kind) in tokens {
        match kind {
            TokenKind::Literal(s) => result.push_str(&s),
            TokenKind::Variable(var) => {
                let resolved = resolve_var(&var, config, ctx, padding);
                result.push_str(&resolved);
            }
        }
    }

    result
}

fn resolve_var(var: &str, _config: &NamingConfig, ctx: &TemplateContext, _padding: usize) -> String {
    let idx = ctx.index as isize;

    match var {
        "filename" => ctx.filename.clone(),
        "ext" => ctx.extension.clone(),
        "num" => idx.to_string(),
        "num2" => format!("{:02}", idx),
        "num3" => format!("{:03}", idx),
        "num4" => format!("{:04}", idx),
        "date" => ctx.date.clone(),
        "time" => ctx.time.clone(),
        "start" => TemplateContext::format_time(ctx.start_time),
        "end" => TemplateContext::format_time(ctx.end_time),
        "duration" => format!("{:.1}", ctx.duration),
        "chapter" => ctx.chapter.clone().unwrap_or_default(),
        "part_label" => ctx.part_label.clone().unwrap_or_else(|| "Part".to_string()),
        "resolution" => ctx.resolution.clone().unwrap_or_default(),
        "width" => ctx.width.map(|w| w.to_string()).unwrap_or_default(),
        "height" => ctx.height.map(|h| h.to_string()).unwrap_or_default(),
        "folder" => ctx.folder.clone().unwrap_or_default(),
        "playlist" => ctx.playlist.clone().unwrap_or_default(),
        "playlist_index" => ctx.playlist_index.map(|i| i.to_string()).unwrap_or_default(),
        "original_num" => ctx.original_num.clone().unwrap_or_default(),
        "video_count" => ctx.video_count.map(|c| c.to_string()).unwrap_or_default(),
        "total_duration" => ctx
            .total_duration
            .map(TemplateContext::format_total_duration)
            .unwrap_or_default(),
        "lang" => ctx.lang.clone().unwrap_or_default(),
        "lang_name" => ctx.lang_name.clone().unwrap_or_default(),
        _ => format!("{{{}}}", var),
    }
}

pub fn resolve_batch(
    template: &str,
    config: &NamingConfig,
    contexts: &[TemplateContext],
) -> Vec<String> {
    contexts
        .iter()
        .map(|ctx| resolve_template(template, config, ctx))
        .collect()
}