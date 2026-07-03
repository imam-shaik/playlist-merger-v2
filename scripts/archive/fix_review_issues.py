#!/usr/bin/env python3
"""Fix review findings: FastMkv premature success, build.rs simplification, unused field warning."""
import os

changes = 0

# ── Fix 1: FastMkv premature success ─────────────────────────────────────
merge_path = os.path.join('src-tauri', 'src', 'commands', 'merge.rs')
with open(merge_path, 'r', encoding='utf-8') as f:
    content = f.read()

# Find: the premature end_forensic_log(Success) before run_fast_mkv_pipeline
old_fastmkv = '''        // Finalize forensic log before branching to FastMkv pipeline
        crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&request.output_path));
        return run_fast_mkv_pipeline('''

new_fastmkv = '''        let fastmkv_result = run_fast_mkv_pipeline('''

if old_fastmkv in content:
    content = content.replace(old_fastmkv, new_fastmkv, 1)
    changes += 1
    print("FIXED: Removed premature FastMkv Success")
else:
    print("NOT FOUND: FastMkv premature success pattern")

# Now find the closing of run_fast_mkv_pipeline - we need to match the end
# The original was:
#   return run_fast_mkv_pipeline(
#       ...,        // 8 parameters over multiple lines
#   ).await;
#
# After fix:
#   let fastmkv_result = run_fast_mkv_pipeline(
#       ...,        // same 8 parameters
#   ).await;
#
# We need to change:
#   let fastmkv_result = run_fast_mkv_pipeline(...).await;
# to:
#   let fastmkv_result = run_fast_mkv_pipeline(...).await;
#   match &fastmkv_result {
#       Ok(_) => crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&request.output_path)),
#       Err(e) => crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Failed, Some(e), Some(&request.output_path)),
#   }
#   return fastmkv_result;

# Find the .await; line after run_fast_mkv_pipeline
old_return = '''        ).await;
    }'''

new_return = '''        ).await;
        match &fastmkv_result {
            Ok(_) => crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&request.output_path)),
            Err(e) => crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Failed, Some(e), Some(&request.output_path)),
        }
        return fastmkv_result;
    }'''

# Check: is the old pattern correct? The code context:
#     return run_fast_mkv_pipeline(
#         ...
#     ).await;
# After our fix:
#     let fastmkv_result = run_fast_mkv_pipeline(
#         ...
#     ).await;
# Then we need to add match + return

# Let's find if there's already a "let fastmkv_result" that we added earlier
if 'let fastmkv_result = run_fast_mkv_pipeline(' in content:
    # Find the .await; line that belongs to fastmkv_result
    # Find all occurrences of ).await;\n    }' 
    # Find the one after fastmkv_result
    idx_result = content.find('let fastmkv_result = run_fast_mkv_pipeline(')
    idx_await = content.find(').await;\n    }', idx_result)
    if idx_await >= 0 and 'match &fastmkv_result' not in content[idx_result:idx_await+100]:
        old_await = content[idx_await:idx_await + len(').await;\n    }')]
        new_await = ''').await;
        match &fastmkv_result {
            Ok(_) => crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&request.output_path)),
            Err(e) => crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Failed, Some(e), Some(&request.output_path)),
        }
        return fastmkv_result;
    }'''
        content = content[:idx_await] + new_await + content[idx_await + len(old_await):]
        changes += 1
        print("FIXED: Added match block after FastMkv pipeline call")
    elif 'match &fastmkv_result' in content[idx_result:idx_await+100]:
        print("SKIP: FastMkv match block already exists")
    else:
        print("NOT FOUND: await pattern after fastmkv_result")
else:
    print("NOT FOUND: fastmkv_result in content")

if changes > 0:
    with open(merge_path, 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"\n✅ {changes} FastMkv fix(es) written")


# ── Fix 2: Simplify build.rs timestamp ────────────────────────────────────
build_path = os.path.join('src-tauri', 'build.rs')
with open(build_path, 'r', encoding='utf-8') as f:
    build = f.read()

# Replace the complex timestamp formatting with simple Unix seconds
old_ts_block = '''    let timestamp = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Format as YYYY-MM-DD HH:MM:SS from Unix timestamp
        format_build_timestamp(now)
    };'''

new_ts_block = '''    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();'''

if old_ts_block in build:
    build = build.replace(old_ts_block, new_ts_block, 1)
    changes += 1
    print("FIXED: build.rs - simplified timestamp to Unix seconds")

    # Remove the helper functions
    old_helper = '''
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
'''

    if old_helper in build:
        build = build.replace(old_helper, '', 1)
        print("FIXED: build.rs - removed helper functions")
    else:
        print("NOT FOUND: build.rs helper functions")

    with open(build_path, 'w', encoding='utf-8') as f:
        f.write(build)
    print("FIXED: build.rs written")
else:
    print("SKIP: build.rs - already simplified or pattern not found")


# ── Fix 3: Fix unused log_path field in forensic_log.rs ───────────────────
# Just add #[allow(dead_code)] or use the field in Drop
forensic_path = os.path.join('src-tauri', 'src', 'forensic_log.rs')
with open(forensic_path, 'r', encoding='utf-8') as f:
    forensic = f.read()

# Add suppress for the unused field
old_field = '    log_path: PathBuf,'
new_field = '    #[allow(dead_code)]\n    log_path: PathBuf,'
if old_field in forensic and 'allow(dead_code)' not in forensic:
    forensic = forensic.replace(old_field, new_field, 1)
    with open(forensic_path, 'w', encoding='utf-8') as f:
        f.write(forensic)
    print("FIXED: forensic_log.rs - suppressed unused log_path warning")
else:
    print("SKIP: forensic_log.rs - already fixed")


# ── Fix 4: Update forensic_log.rs to format BUILD_TIMESTAMP at runtime ────
with open(forensic_path, 'r', encoding='utf-8') as f:
    forensic = f.read()

# Replace the env!("BUILD_TIMESTAMP") usage to format Unix timestamp
old_ts_usage = '    let build_ts = option_env!("BUILD_TIMESTAMP").unwrap_or("N/A");'
new_ts_usage = '''    let build_ts = option_env!("BUILD_TIMESTAMP")
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "N/A".to_string());'''

if old_ts_usage in forensic:
    forensic = forensic.replace(old_ts_usage, new_ts_usage, 1)
    with open(forensic_path, 'w', encoding='utf-8') as f:
        f.write(forensic)
    print("FIXED: forensic_log.rs - BUILD_TIMESTAMP now formatted at runtime")
else:
    print("SKIP: forensic_log.rs timestamp usage already fixed")


print("\nDone.")
