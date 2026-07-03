# Fix: Add enable_media_validation guard around validate_input_files call
# When disabled, create an empty MediaValidationReport so downstream code works safely.

OLD = b'    let _ = std::fs::create_dir_all(&quarantine_dir);\n\n    let media_report = validate_input_files(\n        &ffprobe_path_resolved,\n        &ffmpeg_path_resolved,\n        &working_input_files,\n        &quarantine_dir,\n        Some(cancel_flag.clone()),\n    );'

NEW = b'    let _ = std::fs::create_dir_all(&quarantine_dir);\n    // Check if media validation is enabled (default: true)\n    let validation_settings = crate::services::settings::load_settings_internal();\n    let media_report = if validation_settings.enable_media_validation {\n        validate_input_files(\n            &ffprobe_path_resolved,\n            &ffmpeg_path_resolved,\n            &working_input_files,\n            &quarantine_dir,\n            Some(cancel_flag.clone()),\n        )\n    } else {\n        log::info!("[MEDIA_VALIDATION] Skipped (disabled in settings)");\n        crate::ffmpeg::media_validation_engine::MediaValidationReport::default()\n    };'

with open('src-tauri/src/commands/merge.rs', 'rb') as f:
    content = f.read()

idx = content.find(OLD)
if idx >= 0:
    print(f"Found pattern at byte offset {idx}")
    content = content[:idx] + NEW + content[idx+len(OLD):]
    with open('src-tauri/src/commands/merge.rs', 'wb') as f:
        f.write(content)
    print("SUCCESS: Added enable_media_validation guard")
else:
    print("FAILED: Pattern not found")
    # Try with different whitespace
    # Read first occurrence of validate_input_files
    idx2 = content.find(b'validate_input_files(')
    if idx2 >= 0:
        print(f"'validate_input_files(' found at {idx2}")
        start = max(0, idx2 - 60)
        end = min(len(content), idx2 + 150)
        print(f"Context: {content[start:end]}")
