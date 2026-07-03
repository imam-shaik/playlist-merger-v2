#!/usr/bin/env python3
"""Insert the conditional smartMkvBreakdown emission in merge.rs"""

import os

MERGE_RS = os.path.join(os.path.dirname(__file__), '..', 'src', 'commands', 'merge.rs')

with open(MERGE_RS, 'r', encoding='utf-8') as f:
    content = f.read()

# Find the normalizationPlan emission closure
# Pattern: after classifications line, the object closes with },
# then })); ends the emit call, then let mut already_normalized
marker = 'let mut already_normalized:'
idx = content.find(marker)
if idx < 0:
    print("ERROR: Could not find 'let mut already_normalized'")
    exit(1)

# Look backwards from idx to find the })); that closes the emit call
close_emit = content.rfind('}));', idx - 200, idx)
if close_emit < 0:
    print("ERROR: Could not find emit closure")
    exit(1)

# The emit block ends with }));\n\n        let mut already_normalized:
# We insert between })); and the blank line before already_normalized

insert_block = """
        // ── Smart MKV Dashboard: emit breakdown data (only in SmartMkv mode) ──
        if let Some(ref smb) = smart_mkv_breakdown {
            let _ = app_handle.emit("merge-progress", &serde_json::json!({
                "jobId": request.job_id,
                "progress": {
                    "phase": "normalizing",
                    "smartMkvBreakdown": {
                        "willNormalize": normalize_count,
                        "willRemux": remux_count,
                        "willSkip": skip_count,
                        "totalFiles": total_input_count,
                        "categories": {
                            "normalize": smb.normalize.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                            "remux": smb.remux.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                            "skip": smb.skip.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                        }
                    }
                }
            }));
        }
"""

# Find the exact insertion point: after })); and before the blank line + already_normalized
insert_point = content.rfind('\n', close_emit, close_emit + 3)
if insert_point < 0:
    insert_point = close_emit + 4

content = content[:insert_point] + insert_block + content[insert_point:]

with open(MERGE_RS, 'w', encoding='utf-8') as f:
    f.write(content)

print("Successfully inserted smartMkvBreakdown emission")
