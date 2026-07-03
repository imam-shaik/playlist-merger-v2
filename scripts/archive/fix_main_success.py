#!/usr/bin/env python3
"""Fix the main merge success path in merge.rs."""
import os

path = os.path.join('src-tauri', 'src', 'commands', 'merge.rs')
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# The actual content from codebase - note: crate::logger:: has NO leading whitespace
old = '''                }));
crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));
                log::info!("[EVENT_EMIT] event=merge-complete jobId={} emitted", job_id);'''

new = '''                }));
crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));
crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));
                log::info!("[EVENT_EMIT] event=merge-complete jobId={} emitted", job_id);'''

if old in content:
    content = content.replace(old, new, 1)
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)
    print('FIXED: Main merge success path')
elif 'end_forensic_log(ForensicStatus::Success' in content:
    print('ALREADY FIXED: Main merge success path')
else:
    # Let's find where it is
    idx = content.find('JOB_COMPLETE] Merge completed successfully')
    if idx >= 0:
        start = max(0, idx - 200)
        end = min(len(content), idx + 100)
        snippet = content[start:end]
        print(f'Found at offset {idx}')
        print('Context:')
        print(repr(snippet))
    else:
        print('NOT FOUND: Pattern not in file')
