import re

with open('src-tauri/src/commands/merge.rs', 'rb') as f:
    content = f.read()

# Find the MEDIA_VALIDATION_ENGINE log block
old = b'log::info!("[MEDIA_VALIDATION_ENGINE] Validating {} input files with media validation engine\\n", working_input_files.len());\n        let quarantine_dir = temp_dir.join("quarantine");\n        let _ = std::fs::create_dir_all(&quarantine_dir);\n\n        let media_report = validate_input_files(\n            &ffprobe_path_resolved,\n            &ffmpeg_path_resolved,\n            &working_input_files,\n            &quarantine_dir,\n            Some(cancel_flag.clone()),\n        );'

new = b'// Check if media validation is enabled in settings\n        let validation_settings = crate::services::settings::load_settings_internal();\n        if validation_settings.enable_media_validation {\n            log::info!("[MEDIA_VALIDATION_ENGINE] Validating {} input files with media validation engine\\n", working_input_files.len());\n            let quarantine_dir = temp_dir.join("quarantine");\n            let _ = std::fs::create_dir_all(&quarantine_dir);\n\n            let media_report = validate_input_files(\n                &ffprobe_path_resolved,\n                &ffmpeg_path_resolved,\n                &working_input_files,\n                &quarantine_dir,\n                Some(cancel_flag.clone()),\n            );'

idx = content.find(old)
if idx >= 0:
    print(f"Found MEDIA_VALIDATION_ENGINE at byte offset {idx}")
    content = content[:idx] + new + content[idx+len(old):]
    with open('src-tauri/src/commands/merge.rs', 'wb') as f:
        f.write(content)
    print("SUCCESS: Added enable_media_validation guard")
else:
    print("FAILED: Pattern not found - trying fuzzy search")
    # Try to find the pattern with different whitespace
    patterns = [
        b'MEDIA_VALIDATION_ENGINE',
        b'validate_input_files',
    ]
    for p in patterns:
        idx = content.find(p)
        if idx >= 0:
            print(f"Found '{p.decode()}' at offset {idx}")
            # Show context
            start = max(0, idx - 50)
            end = min(len(content), idx + 200)
            print(f"Context: {content[start:end]}")
