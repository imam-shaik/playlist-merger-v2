#!/usr/bin/env python3
"""Insert Phase 6 per-property NORM_BREAKDOWN logging in merge.rs.

The logging goes right after the NORM_FORENSICS per-file breakdown section
and shows a count of which properties triggered normalization.
"""

import os

MERGE_RS = os.path.join(os.path.dirname(__file__), '..', 'src', 'commands', 'merge.rs')

with open(MERGE_RS, 'r', encoding='utf-8') as f:
    content = f.read()

# Find the NORM_FORENSICS per-file breakdown footer
# Pattern: "[NORM_FORENSICS] ---"
# Followed by "[NORM_FORENSICS] NORMALIZATION PLAN:"
# We insert right after the per-file breakdown footer

marker = '[NORM_FORENSICS] \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500'
idx = content.find(marker)
if idx < 0:
    # Try ASCII version
    marker_ascii = '[NORM_FORENSICS] ----------------------------------------------------'
    idx = content.find(marker_ascii)
    if idx < 0:
        print("ERROR: Could not find NORM_FORENSICS footer")
        # Try to find the end of the per-file breakdown
        for search in ['PER-FILE BREAKDOWN', 'Normalization Count by Type', 'normalCount']:
            pos = content.find(search)
            if pos >= 0:
                print(f"  Found '{search}' at position {pos}")
        exit(1)

# Find the end of this line
eol = content.find('\n', idx)
if eol < 0:
    print("ERROR: Could not find end of line")
    exit(1)

# The insertion point is right after the separator line
# We insert right after \n following the separator
insert_point = eol + 1

# Build the Phase 6 insertion
insertion = """    // ── PHASE 6: Per-Property Normalization Breakdown ────────────────────────
    // Logs which specific properties triggered normalization for each file.
    // This helps users understand WHY each file needs normalization.
    if !analysis.outliers.is_empty() {
        let mut norm_prop_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for o in &analysis.outliers {
            if o.normalization_type != crate::ffmpeg::normalization::NormalizationType::None {
                *norm_prop_counts.entry(o.property.as_str()).or_insert(0) += 1;
            }
        }
        // Log top properties sorted by count
        let mut sorted_norm_props: Vec<_> = norm_prop_counts.into_iter().collect();
        sorted_norm_props.sort_by(|a, b| b.1.cmp(&a.1));
        log::info!("[NORM_BREAKDOWN] Per-property normalization triggers:");
        log::info!("[NORM_BREAKDOWN]   {:>25} | {:>5} files", "Property", "Count");
        log::info!("[NORM_BREAKDOWN]   {:->25} | {:->5}", "---", "---");
        for (prop, count) in &sorted_norm_props {
            log::info!("[NORM_BREAKDOWN]   {:>25} | {:>5}", prop, count);
        }
        log::info!("[NORM_BREAKDOWN]   {:->25} | {:->5}", "---", "---");
        log::info!("[NORM_BREAKDOWN]   {:>25} | {:>5} total triggers", "TOTAL", analysis.outliers.len());
    }
    if !analysis.audio_outliers.is_empty() {
        let mut audio_prop_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for ao in &analysis.audio_outliers {
            *audio_prop_counts.entry(ao.audio_type.label().to_string()).or_insert(0) += 1;
        }
        let mut sorted_audio_props: Vec<_> = audio_prop_counts.into_iter().collect();
        sorted_audio_props.sort_by(|a, b| b.1.cmp(&a.1));
        log::info!("[NORM_BREAKDOWN] Audio outlier types:");
        for (label, count) in &sorted_audio_props {
            log::info!("[NORM_BREAKDOWN]   {}: {}", label, count);
        }
    }
"""

content = content[:insert_point] + insertion + content[insert_point:]

with open(MERGE_RS, 'w', encoding='utf-8') as f:
    f.write(content)

print("Phase 6 implemented successfully!")
print(f"Inserted at position {insert_point}")
