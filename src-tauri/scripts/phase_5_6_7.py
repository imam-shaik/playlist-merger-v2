#!/usr/bin/env python3
"""Implement Phases 5, 6, and 7 for Smart MKV optimization in merge.rs.

Phase 5: Smart MKV Dashboard - emit smartMkvAnalysis breakdown data
Phase 6: Per-property breakdown logging in normalization loops  
Phase 7: Parallel normalization using tokio::JoinSet
"""

import sys
import os

MERGE_RS = os.path.join(os.path.dirname(__file__), '..', 'src', 'commands', 'merge.rs')

with open(MERGE_RS, 'r', encoding='utf-8') as f:
    content = f.read()

# ======================================================================
# First, add the compute_smart_mkv_breakdown function
# This goes before the `start_merge` function
# ======================================================================

# Find where the RealWorkloadForensics report ends (near the start)
# and insert the helper function before start_merge
fn_search = 'pub async fn start_merge('
fn_pos = content.find(fn_search)
if fn_pos < 0:
    print("ERROR: Could not find start_merge")
    sys.exit(1)

# Find the last newline before start_merge
last_newline = content.rfind('\n', 0, fn_pos)

helper_fn = '''

/// Smart MKV analysis breakdown: categorize which properties fall into
/// normalize, remux, or skip categories.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SmartMkvBreakdown {
    pub normalize: Vec<PropertyCount>,
    pub remux: Vec<PropertyCount>,
    pub skip: Vec<PropertyCount>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PropertyCount {
    pub property: String,
    pub count: usize,
}

/// Compute a 3-category breakdown of Smart MKV analysis.
/// Uses the filter function to determine which outliers are MKV-safe.
pub fn compute_smart_mkv_breakdown(analysis: &crate::ffmpeg::normalization::ProfileAnalysis) 
    -> SmartMkvBreakdown 
{
    use crate::ffmpeg::normalization::filter_outliers_for_mkv;
    use std::collections::HashMap;

    let (filtered_outliers, filtered_audio_outliers) = filter_outliers_for_mkv(analysis);

    // Build sets of kept indices for quick lookup
    let kept_normalize: std::collections::HashSet<usize> = filtered_outliers.iter()
        .map(|o| o.index).collect();
    let kept_audio: std::collections::HashSet<usize> = filtered_audio_outliers.iter()
        .map(|a| a.index).collect();

    // Categorize removed outliers by property
    let mut normalize_props: HashMap<&str, usize> = HashMap::new();
    let mut remux_props: HashMap<&str, usize> = HashMap::new();
    let mut skip_props: HashMap<&str, usize> = HashMap::new();

    // Outliers that were KEPT need normalize or remux
    for o in &analysis.outliers {
        if kept_normalize.contains(&o.index) {
            let prop = o.property.as_str();
            if prop == "time_base" {
                *remux_props.entry(prop).or_insert(0) += 1;
            } else {
                *normalize_props.entry(prop).or_insert(0) += 1;
            }
        } else {
            *skip_props.entry(o.property.as_str()).or_insert(0) += 1;
        }
    }

    // Map property names to user-friendly labels
    let prop_labels = |p: &str| -> &str {
        match p {
            "v_codec" => "Video codec",
            "v_profile" => "Video profile",
            "resolution" => "Resolution",
            "v_fps" => "FPS",
            "pixel_format" => "Pixel format",
            "color_space" => "Color space",
            "color_transfer" => "Color transfer",
            "color_primaries" => "Color primaries",
            "v_bit_depth" => "Bit depth",
            "field_order" => "Interlacing",
            "rotation" => "Rotation",
            "dar" => "DAR",
            "hdr" => "HDR",
            "frame_rate_type" => "VFR",
            "a_sample_rate" => "Sample rate",
            "a_channels" => "Audio channels",
            "a_profile" => "AAC profile",
            "a_codec" => "Audio codec",
            "a_channel_layout" => "Channel layout",
            "a_bit_depth" => "Audio bit depth",
            "a_language" => "Audio language",
            "container_format" => "Container",
            "num_video_streams" => "Multiple video streams",
            "num_audio_streams" => "Multiple audio streams",
            "missing_video_stream" => "Missing video stream",
            "missing_audio_stream" => "Missing audio stream",
            "audio_duration_drift" => "Audio duration drift",
            "audio_start_offset" => "Audio start offset",
            "time_base" => "Timebase",
            _ => p,
        }
    };

    let mut normalize: Vec<PropertyCount> = normalize_props.into_iter()
        .map(|(p, c)| PropertyCount { property: prop_labels(p).to_string(), count: c })
        .collect();
    normalize.sort_by(|a, b| b.count.cmp(&a.count));

    let mut remux: Vec<PropertyCount> = remux_props.into_iter()
        .map(|(p, c)| PropertyCount { property: prop_labels(p).to_string(), count: c })
        .collect();
    remux.sort_by(|a, b| b.count.cmp(&a.count));

    let mut skip: Vec<PropertyCount> = skip_props.into_iter()
        .map(|(p, c)| PropertyCount { property: prop_labels(p).to_string(), count: c })
        .collect();
    skip.sort_by(|a, b| b.count.cmp(&a.count));

    SmartMkvBreakdown { normalize, remux, skip }
}
'''

content = content[:last_newline] + helper_fn + content[last_newline:]

# ======================================================================
# PHASE 5: Emit smartMkvAnalysis in the progress event
# Find the normalizationPlan emission (around "normalizationPlan": { block)
# and add smartMkvAnalysis field after it
# ======================================================================

# Find where normalizationPlan is emitted in the progress event
plan_search = '"normalizationPlan": {'
plan_pos = content.find(plan_search)
if plan_pos < 0:
    print("ERROR: Could not find normalizationPlan emission")
    sys.exit(1)

# Find the closing of the normalizationPlan object - it ends with },
# then the next field or the closing of the progress object
plan_end = content.find('}', plan_pos)
# The normalizationPlan object ends with }, so find the }
# after the classifications array closes with ] and }
close_search = '],\n                },\n            }\n        }));'
close_pos = content.find(close_search, plan_pos)
if close_pos < 0:
    # Try alternative closing pattern
    close_search2 = 'classifications'
    close_pos2 = content.find(close_search2, plan_pos)
    if close_pos2 < 0:
        print("ERROR: Could not find normalizationPlan closing")
        sys.exit(1)
    # Find the } that closes the whole progress object after classifications
    close_pos = content.find('}\n        }));', close_pos2)
    if close_pos < 0:
        print("ERROR: Could not find progress object closing")
        sys.exit(1)

# Now we need to insert the smartMkvAnalysis field inside the progress object
# Find the last } before the closing })); - that closes normalizationPlan
# We want to insert AFTER normalizationPlan's closing }
last_bracket = content.rfind('}', plan_pos, close_pos)
if last_bracket > 0:
    # Insert smartMkvAnalysis after the normalizationPlan closing brace
    # The pattern is: "},\n            }\n        }));"
    # We want to add: "},\n            \"smartMkvAnalysis\": { ... },\n            }\n        }));"
    # Actually, let's find where to insert: after the } that closes normalizationPlan
    # and before the } that closes the progress object
    
    # Look for: "}\n        }));" after classifications
    pattern = '}\n        }));'
    pattern_pos = content.find(pattern, plan_pos)
    if pattern_pos > 0:
        smart_mkv_field = ''',
                "smartMkvAnalysis": {
                    "totalFiles": total_input_count,
                    "willNormalize": normalize_count,
                    "willRemux": remux_count,
                    "willSkip": skip_count,
                    "categories": {
                        "normalize": [{"property": c.property, "count": c.count} for c in smart_mkv_breakdown.normalize],
                        "remux": [{"property": c.property, "count": c.count} for c in smart_mkv_breakdown.remux],
                        "skip": [{"property": c.property, "count": c.count} for c in smart_mkv_breakdown.skip],
                    }
                }'''
        # Actually, we need to serialize the breakdown properly for JSON
        # The smart_mkv_breakdown is already on the Rust side
        # We need to add it to the JSON emitted via app_handle.emit
        # Let's use serde_json::to_value on the breakdown
        
        # Find the emit call and add the breakdown to it
        # Pattern: app_handle.emit("merge-progress", &serde_json::json!({
        emit_search = 'app_handle.emit("merge-progress"'
        
        # But this is complex. Let's take a simpler approach:
        # We'll look for the specific emission block that contains normalizationPlan
        # and add the smartMkvAnalysis next to it
        
        print(f"Found normalizationPlan at position {plan_pos}")
        print(f"Pattern at {pattern_pos}")
        print(f"Context: {content[plan_pos-100:plan_pos+50]}")

# ======================================================================
# PHASE 7: Parallel normalization using tokio::JoinSet
# Replace the sequential profile norm loop (starts around line 2838)
# and audio norm loop with parallel execution
# ======================================================================

with open(MERGE_RS, 'w', encoding='utf-8') as f:
    f.write(content)

print("Phase 5/6/7 script written to file (partial)")
print("The merge.rs file has been updated with the helper function")
