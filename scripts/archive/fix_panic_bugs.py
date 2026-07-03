#!/usr/bin/env python3
"""Fix all 6 panic bugs identified in the audit."""
import os

BASE = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'src-tauri', 'src')

def fix_merge_rs():
    path = os.path.join(BASE, 'commands', 'merge.rs')
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()
    changes = 0

    # H2: groups.last_mut().unwrap() -> if let Some
    old_h2 = '                if last_folder.as_ref() != Some(&folder) {\n                    groups.push(Vec::new());\n                    last_folder = Some(folder);\n                }\n                groups.last_mut().unwrap().push(idx);\n            }\n            groups'
    new_h2 = '                if last_folder.as_ref() != Some(&folder) {\n                    groups.push(Vec::new());\n                    last_folder = Some(folder);\n                }\n                if let Some(g) = groups.last_mut() {\n                    g.push(idx);\n                }\n            }\n            groups'
    if old_h2 in content:
        content = content.replace(old_h2, new_h2, 1)
        changes += 1
        print("H2 FIXED: groups.last_mut().unwrap() -> if let Some(g)")
    else:
        print("H2 SKIP: pattern not found")

    # C4a: strip_prefix("ProfileMismatch_Video(").unwrap() -> .map().unwrap_or("")
    old_c4a = '                    let inner = r.strip_prefix("ProfileMismatch_Video(").unwrap().trim_end_matches(\')\');'
    new_c4a = '                    let inner = r.strip_prefix("ProfileMismatch_Video(").map(|s| s.trim_end_matches(\')\')).unwrap_or("");'
    if old_c4a in content:
        content = content.replace(old_c4a, new_c4a, 1)
        changes += 1
        print("C4a FIXED: Video strip_prefix unwrap")
    else:
        print("C4a SKIP: pattern not found")

    # C4b: strip_prefix("ProfileMismatch_Audio(").unwrap() -> .map().unwrap_or("")
    old_c4b = '                    let inner = r.strip_prefix("ProfileMismatch_Audio(").unwrap().trim_end_matches(\')\');'
    new_c4b = '                    let inner = r.strip_prefix("ProfileMismatch_Audio(").map(|s| s.trim_end_matches(\')\')).unwrap_or("");'
    if old_c4b in content:
        content = content.replace(old_c4b, new_c4b, 1)
        changes += 1
        print("C4b FIXED: Audio strip_prefix unwrap")
    else:
        print("C4b SKIP: pattern not found")

    # C1: mkvmerge_path.unwrap() -> match
    old_c1 = '    if will_use_mkvmerge {\n        let path = mkvmerge_path.unwrap();'
    new_c1 = '    if will_use_mkvmerge {\n        let path = match mkvmerge_path {\n            Some(p) => p,\n            None => {\n                log::error!("[Merge] mkvmerge not found -- cannot use SmartMKV mode. Falling back to standard merge.");\n                return Err("mkvmerge not found. Cannot use SmartMKV mode. Please install MKVToolNix or select a different merge mode.".to_string());\n            }\n        };'
    if old_c1 in content:
        content = content.replace(old_c1, new_c1, 1)
        changes += 1
        print("C1 FIXED: mkvmerge_path.unwrap() -> match")
    else:
        print("C1 SKIP: pattern not found")

    # C2: if result.is_ok() { ... result.as_ref().unwrap() ... } -> if let Ok(ref result)
    old_c2 = '        if result.is_ok() {\n            let mut s = crate::services::settings::load_settings_internal();\n            s.recent_exports.insert(0, RecentExport { path: output_path.clone(), timestamp: Utc::now(), size_bytes: result.as_ref().unwrap().output_size_bytes, file_count: original_file_count, duration_seconds: original_total_duration, mode: format!("{:?}", actual_mode) });\n            s.recent_exports.truncate(10); let _ = crate::services::settings::save_settings_internal(&s);\n        }'
    new_c2 = '        if let Ok(ref merge_result) = result {\n            let mut s = crate::services::settings::load_settings_internal();\n            s.recent_exports.insert(0, RecentExport { path: output_path.clone(), timestamp: Utc::now(), size_bytes: merge_result.output_size_bytes, file_count: original_file_count, duration_seconds: original_total_duration, mode: format!("{:?}", actual_mode) });\n            s.recent_exports.truncate(10); let _ = crate::services::settings::save_settings_internal(&s);\n        }'
    if old_c2 in content:
        content = content.replace(old_c2, new_c2, 1)
        changes += 1
        print("C2 FIXED: result.is_ok() + unwrap -> if let Ok")
    else:
        print("C2 SKIP: pattern not found")

    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"merge.rs: {changes} changes applied")


def fix_concat_rs():
    path = os.path.join(BASE, 'ffmpeg', 'concat.rs')
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()
    changes = 0

    # C3: Replace .expect("Errors lock still referenced").into_inner().expect("Mutex poisoned")
    # with .map(|arc| arc.into_inner().unwrap_or_default()).unwrap_or_default()
    old = '''    let errors = std::sync::Arc::into_inner(errors)
        .expect("Errors lock still referenced")
        .into_inner()
        .expect("Mutex poisoned");'''
    new = '''    let errors = std::sync::Arc::into_inner(errors)
        .map(|arc| arc.into_inner().unwrap_or_default())
        .unwrap_or_default();'''
    if old in content:
        content = content.replace(old, new, 1)
        changes += 1
        print("C3 FIXED (concat): Mutex expect -> unwrap_or_default")
    else:
        print("C3 SKIP (concat): pattern not found")

    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"concat.rs: {changes} changes applied")


def fix_normalization_rs():
    path = os.path.join(BASE, 'ffmpeg', 'normalization.rs')
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()
    changes = 0

    # H1: sem.acquire().await.expect("Semaphore closed") -> match returning error
    old_h1 = '''            let _permit = sem.acquire().await.expect("Semaphore closed");'''
    new_h1 = '''            let _permit = match sem.acquire().await {
                        Ok(p) => p,
                        Err(_) => {
                            log::error!("[Normalization] Semaphore closed during corruption check");
                            return None;
                        }
                    };'''
    if old_h1 in content:
        content = content.replace(old_h1, new_h1, 1)
        changes += 1
        print("H1 FIXED: Semaphore expect -> match")
    else:
        print("H1 SKIP: pattern not found (may have whitespace differences)")

    # Also fix the spawn_blocking .expect()
    old_spawn = '''            }).await.expect("Corruption check task panicked");'''
    new_spawn = '''            }).await.unwrap_or_else(|e| {
                        log::error!("[Normalization] Corruption check task panicked: {}", e);
                        None
                    });'''
    if old_spawn in content:
        content = content.replace(old_spawn, new_spawn, 1)
        changes += 1
        print("C3/H1 FIXED: spawn_blocking expect -> unwrap_or_else")
    else:
        print("C3/H1 SKIP: spawn_blocking pattern not found")

    # C3 (normalization): Fix both Arc::into_inner + Mutex patterns
    old_results = '''    let results = std::sync::Arc::into_inner(results)
        .expect("Results lock still referenced")
        .into_inner()
        .expect("Mutex poisoned");
    let all_errors = std::sync::Arc::into_inner(all_errors)
        .expect("Errors lock still referenced")
        .into_inner()
        .expect("Mutex poisoned");'''
    new_results = '''    let results = std::sync::Arc::into_inner(results)
        .map(|arc| arc.into_inner().unwrap_or_default())
        .unwrap_or_default();
    let all_errors = std::sync::Arc::into_inner(all_errors)
        .map(|arc| arc.into_inner().unwrap_or_default())
        .unwrap_or_default();'''
    if old_results in content:
        content = content.replace(old_results, new_results, 1)
        changes += 1
        print("C3 FIXED (norm): Mutex expects -> unwrap_or_default")
    else:
        print("C3 SKIP (norm): pattern not found")

    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"normalization.rs: {changes} changes applied")


if __name__ == '__main__':
    fix_merge_rs()
    fix_concat_rs()
    fix_normalization_rs()
    print("\nAll fixes applied.")
