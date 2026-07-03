#!/usr/bin/env python3
"""Add mkvmerge_succeeded flag and gate METADATA_FIX section on it."""
import sys

# Force UTF-8 for stdout
sys.stdout.reconfigure(encoding='utf-8')

filepath = 'src-tauri/src/commands/merge.rs'

with open(filepath, 'r', encoding='utf-8') as f:
    content = f.read()

# === Change 3: Gate the METADATA_FIX section on mkvmerge_succeeded ===
# Find the METADATA_FIX section start
fix_marker = '        // ── Post-mkvmerge container metadata fix ──'
fix_pos = content.find(fix_marker)

if fix_pos == -1:
    print("ERROR: Could not find METADATA_FIX marker")
    sys.exit(1)

# Change the `if let Some(Ok(ref info))` to be gated on mkvmerge_succeeded
old_fix_gate = '        if let Some(Ok(ref info)) = &output_probe_result {'
new_fix_gate = '        if mkvmerge_succeeded {\n            if let Some(Ok(ref info)) = &output_probe_result {'

content = content.replace(old_fix_gate, new_fix_gate, 1)
print("Change 3a: Gated METADATA_FIX on mkvmerge_succeeded")

# Now fix the closing: we need to add an extra `            }` before the closing `        }`
# The original closing pattern was:
#         }
# (blank line)
#         // Track validation start time
#
# It should become:
#             }
#         }
# (blank line)
#         // Track validation start time

old_closing = '        }\n\n        // Track validation start time for forensic timing'
new_closing = '            }\n        }\n\n        // Track validation start time for forensic timing'

if old_closing in content:
    content = content.replace(old_closing, new_closing, 1)
    print("Change 3b: Added closing brace for mkvmerge_succeeded gate")
else:
    # Try alternative pattern - maybe there's an extra blank line or different whitespace
    # Search for the pattern with regex-like flexibility
    import re
    pattern = r'        \}\n\s*\n        // Track validation start time for forensic timing'
    match = re.search(pattern, content)
    if match:
        old = match.group(0)
        new = '            }\n        }\n\n        // Track validation start time for forensic timing'
        content = content.replace(old, new, 1)
        print("Change 3b: Added closing brace (regex match)")
    else:
        print(f"WARNING: Could not find closing pattern. Searching manually...")
        # Find the METADATA_FIX end marker and look for the closing brace after it
        end_marker = 'log::info!("[METADATA_FIX] ═══════════════════════════════════════════════════════════");'
        end_pos = content.find(end_marker, fix_pos)
        if end_pos != -1:
            # After the end marker, find the closing `        }`
            remaining = content[end_pos + len(end_marker):]
            close_marker = '        }'
            close_pos = remaining.find(close_marker)
            if close_pos != -1:
                actual_pos = end_pos + len(end_marker) + close_pos
                # Check what's after this
                after = content[actual_pos:actual_pos+80]
                j = actual_pos
                # Replace this `        }` with `            }\n        }`
                content = content[:j] + '            }\n        ' + content[j+8:]
                print("Change 3b: Added closing brace (manual match)")
            else:
                print("ERROR: Could not find closing brace")
                sys.exit(1)
        else:
            print("ERROR: Could not find METADATA_FIX end marker")
            sys.exit(1)

with open(filepath, 'w', encoding='utf-8') as f:
    f.write(content)

print("SUCCESS: All changes applied")
