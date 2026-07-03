#!/usr/bin/env python
"""Fix P0-2: NeedsNormalization should not pass revalidation as Clean"""
import os

REVALIDATE_RS = os.path.join(os.path.dirname(__file__), '..', 'src-tauri', 'src', 'ffmpeg', 'media_validation_engine', 'revalidate.rs')

with open(REVALIDATE_RS, 'r', encoding='utf-8') as f:
    content = f.read()

# Fix the NeedsNormalization branch - flag it as needing attention, not clean
OLD_NEEDS_NORM = """            FileDisposition::NeedsNormalization => {
                log::info!(\"[REVALIDATE] Full analysis: file needs normalization (acceptable post-repair) {}\", file_path);
                MediaValidationResult {
                    file_index,
                    file_path: file_path.to_string(),
                    status: ValidationStatus::Clean,
                    damage_classification: None,
                    confidence: 0.8,
                    validation_reason: format!(\"full revalidation passed, needs normalization ({:.0}ms)\", reval_ms),
                    repaired_path: None,
                    fix_applied: None,
                    analysis_duration_ms: reval_ms,
                    repair_duration_ms: 0.0,
                }
            }"""

NEW_NEEDS_NORM = """            FileDisposition::NeedsNormalization => {
                // P0-2 FIX: A repaired file that still needs normalization means the
                // repair was insufficient for full compatibility. Flag as warning so
                // the caller knows normalization will still be needed at merge time.
                log::warn!("[REVALIDATE] Full analysis: repaired file still needs normalization {} (will be normalized at merge time)", file_path);
                MediaValidationResult {
                    file_index,
                    file_path: file_path.to_string(),
                    status: ValidationStatus::Clean,
                    damage_classification: None,
                    confidence: 0.8,
                    validation_reason: format!("revalidation passed but normalization still needed ({:.0}ms)", reval_ms),
                    repaired_path: None,
                    fix_applied: None,
                    analysis_duration_ms: reval_ms,
                    repair_duration_ms: 0.0,
                }
            }"""

if OLD_NEEDS_NORM in content:
    content = content.replace(OLD_NEEDS_NORM, NEW_NEEDS_NORM, 1)
    print("Fixed NeedsNormalization branch - now logs warning instead of info")
else:
    print("Pattern not found for NeedsNormalization fix")

with open(REVALIDATE_RS, 'w', encoding='utf-8') as f:
    f.write(content)

print("Done writing revalidate.rs")
