#!/usr/bin/env python3
"""Fix 3 issues from code review of P0 fixes."""

def read_file(path):
    with open(path, 'rb') as f:
        return f.read()

def write_file(path, content):
    with open(path, 'wb') as f:
        f.write(content)


def main():
    fixes_applied = 0

    # ═══════════════════════════════════════════════════════════════════
    # FIX 1: pipeline.rs - Don't re-fill final_path for quarantined files
    # ═══════════════════════════════════════════════════════════════════
    path1 = 'src-tauri/src/ffmpeg/media_validation_engine/pipeline.rs'
    content1 = read_file(path1)

    old1 = (
        b'        for state in &mut report.file_states {\n'
        b'            if state.final_path.is_empty() {\n'
        b'                state.final_path = state.original_path.clone();\n'
        b'            }\n'
        b'        }'
    )
    new1 = (
        b'        for state in &mut report.file_states {\n'
        b'            if state.final_path.is_empty() '
        b'&& state.repair_status != RepairStatus::Quarantined {\n'
        b'                state.final_path = state.original_path.clone();\n'
        b'            }\n'
        b'        }'
    )

    ok1 = False
    if old1 in content1:
        content1 = content1.replace(old1, new1, 1)
        write_file(path1, content1)
        ok1 = True
        fixes_applied += 1
        print("FIX 1 (pipeline.rs fallback): OK")
    elif new1 in content1:
        ok1 = True
        print("FIX 1 (pipeline.rs fallback): already applied")
    else:
        print("FIX 1 (pipeline.rs fallback): FAILED - pattern not found")
        for line in content1.split(b'\n'):
            if b'final_path.is_empty()' in line:
                print(f"  Found: {line[:120]}")

    # ═══════════════════════════════════════════════════════════════════
    # FIX 2 & 3: merge.rs changes
    # ═══════════════════════════════════════════════════════════════════
    path2 = 'src-tauri/src/commands/merge.rs'
    content2 = read_file(path2)

    # ── FIX 2a: Add Arc creation BEFORE media validation ──
    # The marker is:  `", );\r\n\r\n    // ── MEDIA VALIDATION ENGINE`
    # where ── is U+2500 (box-drawing) encoded as \xe2\x94\x80
    marker = (
        b'    log::info!("[STAGE_TIMING] MEDIA_VALIDATION | start", );\r\n'
        b'\r\n'
        b'    // \xe2\x94\x80\xe2\x94\x80 MEDIA VALIDATION ENGINE'
    )

    insert_block = (
        b'    log::info!("[STAGE_TIMING] MEDIA_VALIDATION | start", );\r\n'
        b'\r\n'
        b'    // -- P0-2: Initialize temp file tracking BEFORE media validation --\r\n'
        b'    let temp_norm_files_arc = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));\r\n'
        b'    let temp_registry = TempFileRegistry {\r\n'
        b'        sub: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),\r\n'
        b'    };\r\n'
        b'\r\n'
        b'    // \xe2\x94\x80\xe2\x94\x80 MEDIA VALIDATION ENGINE'
    )

    fix2a = False
    if marker in content2:
        content2 = content2.replace(marker, insert_block, 1)
        fix2a = True
        fixes_applied += 1
        print("FIX 2a (moved Arc creation): OK")
    elif b'P0-2: Initialize temp file tracking BEFORE media validation' in content2:
        fix2a = True
        print("FIX 2a (moved Arc creation): already applied")
    else:
        print("FIX 2a: FAILED - marker not found")
        # Try to find similar text
        idx = content2.find(b'MEDIA VALIDATION ENGINE')
        if idx >= 0:
            snippet = content2[max(0,idx-50):idx+100]
            print(f"  Found at offset {idx}, context: {snippet[:150]}")

    # ── FIX 2b: Remove duplicate temp_norm_files_arc and temp_registry creation ──
    old2b = b'    let temp_norm_files_arc = Arc::new(Mutex::new(Vec::new()));\n    let mut cleanup_guard = TempCleanup::new(Arc::new(temp_registry), Arc::clone(&temp_norm_files_arc));'
    new2b = b'    let mut cleanup_guard = TempCleanup::new(Arc::new(temp_registry), Arc::clone(&temp_norm_files_arc));'

    fix2b = False
    count_instances = content2.count(b'let temp_norm_files_arc =')
    if count_instances > 1:
        if old2b in content2:
            content2 = content2.replace(old2b, new2b, 1)
            fix2b = True
            fixes_applied += 1
            print("FIX 2b (removed duplicate): OK")
        elif new2b in content2:
            fix2b = True
            print("FIX 2b (removed duplicate): already applied")
        else:
            print(f"FIX 2b: FAILED - {count_instances} instances but old pattern not found")
    elif count_instances == 1:
        fix2b = True
        print(f"FIX 2b: 1 instance (moved, no duplicate) OK")
    else:
        print("FIX 2b: FAILED - no instances found")

    # ── FIX 3: P0-4 provenance log - path-based matching ──
    old3 = (
        b'        let (source, method_str, revalidated_str) = if media_report.file_results.iter()'
        b'.any(|r| r.file_index == idx && r.is_fixed()) {\n'
        b'            let method = media_report.file_results.iter()\n'
        b'                .find(|r| r.file_index == idx)\n'
        b'                .and_then(|r| r.fix_applied.as_ref().map(|f| format!("{:?}", f)))\n'
        b'                .unwrap_or_default();\n'
        b'            ("REPAIRED", method, "PASS")\n'
        b'        } else {\n'
        b'            ("ORIGINAL", "none".to_string(), "N/A".to_string())\n'
        b'        };'
    )
    new3 = (
        b'        // P0-4: Match by file PATH (not index) because index-based matching\n'
        b'        // breaks after apply_validation_results() removes quarantined files.\n'
        b'        let (source, method_str, revalidated_str) = '
        b'if let Some(result) = media_report.file_results.iter()'
        b'.find(|r| r.file_path == *fpath) {\n'
        b'            if result.is_fixed() {\n'
        b'                let method = result.fix_applied.as_ref()'
        b'.map(|f| format!("{:?}", f)).unwrap_or_default();\n'
        b'                ("REPAIRED", method, "PASS")\n'
        b'            } else {\n'
        b'                ("ORIGINAL", "none".to_string(), "N/A".to_string())\n'
        b'            }\n'
        b'        } else {\n'
        b'            ("ORIGINAL", "none".to_string(), "N/A".to_string())\n'
        b'        };'
    )

    fix3 = False
    if old3 in content2:
        content2 = content2.replace(old3, new3, 1)
        fix3 = True
        fixes_applied += 1
        print("FIX 3 (P0-4 path-based matching): OK")
    elif b'r.file_path == *fpath' in content2:
        fix3 = True
        print("FIX 3 (P0-4 path-based matching): already applied")
    else:
        print("FIX 3: FAILED - pattern not found")
        idx = content2.find(b'r.file_index == idx')
        if idx >= 0:
            snippet = content2[idx:idx+300]
            print(f"  Found at offset {idx}: {snippet[:200]}")

    # Write merge.rs
    write_file(path2, content2)

    print(f"\n=== RESULT: {fixes_applied}/3 fixes applied ===")
    return fixes_applied == 3

if __name__ == '__main__':
    success = main()
    exit(0 if success else 1)
