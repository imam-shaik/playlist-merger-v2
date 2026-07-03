#!/usr/bin/env python3
"""Fix remaining forensic log integration issues."""
import os

base_dir = os.path.join('src-tauri', 'src', 'commands')

# ── Fix 1: build.rs - replace chrono with std::time ──────────────────────
build_path = os.path.join('src-tauri', 'build.rs')
with open(build_path, 'r', encoding='utf-8') as f:
    build = f.read()

old_ts = "    let timestamp = chrono::Local::now().format(\"%Y-%m-%d %H:%M:%S\").to_string();"
new_ts = """    let timestamp = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Format as YYYY-MM-DD HH:MM:SS from Unix timestamp
        format_build_timestamp(now)
    };"""

if old_ts in build:
    build = build.replace(old_ts, new_ts, 1)
    
    # Add the helper function
    helper = """
fn format_build_timestamp(unix_secs: u64) -> String {
    // Simple UTC timestamp formatting without chrono dependency
    let secs_per_day = 86400u64;
    let days_since_epoch = unix_secs / secs_per_day;
    let time_secs = unix_secs % secs_per_day;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Days since 1970-01-01
    let mut y = 1970i64;
    let mut remaining_days = days_since_epoch as i64;

    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining_days < days_in_year { break; }
        remaining_days -= days_in_year;
        y += 1;
    }

    let months_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 0usize;
    for (i, &md) in months_days.iter().enumerate() {
        if remaining_days < md { m = i + 1; break; }
        remaining_days -= md;
    }
    if m == 0 { m = 12; }

    let d = remaining_days + 1;

    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, hours, minutes, seconds)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
"""
    
    # Insert helper before main()
    build = build.replace("fn main() {", helper + "\nfn main() {")
    
    with open(build_path, 'w', encoding='utf-8') as f:
        f.write(build)
    print("FIXED: build.rs - replaced chrono with std::time")
else:
    print("SKIP: build.rs - already fixed or pattern not found")


# ── Fix 2: FastMkv branch - add end_forensic_log before return ────────────
merge_path = os.path.join(base_dir, 'merge.rs')
with open(merge_path, 'r', encoding='utf-8') as f:
    merge = f.read()

# Find the FastMkv branch
old_fastmkv = '''        log::info!("[Merge:PATH] >>> BRANCHING TO FastMkv pipeline (stream copy, no re-encode) <<<");
        return run_fast_mkv_pipeline(

'''
# Check if already fixed
if 'end_forensic_log(ForensicStatus::Success' in merge and 'BRANCHING TO FastMkv' in merge:
    print("SKIP: merge.rs FastMkv - already fixed")
else:
    # Find the exact text
    idx = merge.find('log::info!("[Merge:PATH] >>> BRANCHING TO FastMkv pipeline')
    if idx >= 0:
        # Get the line up to the return
        start = idx
        # Find the beginning of the line
        line_start = merge.rfind('\n', 0, idx) + 1
        prefix = merge[line_start:idx]
        
        new_fastmkv = prefix + '''log::info!("[Merge:PATH] >>> BRANCHING TO FastMkv pipeline (stream copy, no re-encode) <<<");
        // Finalize forensic log before branching to FastMkv pipeline
        crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&request.output_path));
        return run_fast_mkv_pipeline(
'''
        old = merge[idx:idx + len(prefix + '''log::info!("[Merge:PATH] >>> BRANCHING TO FastMkv pipeline (stream copy, no re-encode) <<<");
        return run_fast_mkv_pipeline(
''')]
        merge = merge.replace(old, new_fastmkv, 1)
        with open(merge_path, 'w', encoding='utf-8') as f:
            f.write(merge)
        print("FIXED: merge.rs FastMkv branch - added end_forensic_log")
    else:
        print("NOT FOUND: FastMkv branch pattern")

# ── Fix 3: Verify main merge success path has correct variable ────────────
with open(merge_path, 'r', encoding='utf-8') as f:
    merge = f.read()

# Check if the variable used in end_forensic_log in main success path is correct
if 'end_forensic_log(ForensicStatus::Success' in merge:
    # Extract the line
    for line in merge.split('\n'):
        if 'end_forensic_log(ForensicStatus::Success' in line:
            print(f"Main merge success path: {line.strip()}")
            break
else:
    print("WARNING: No end_forensic_log(Success) found in merge.rs")

print("\nDone.")
