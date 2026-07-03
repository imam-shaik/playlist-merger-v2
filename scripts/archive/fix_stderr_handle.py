#!/usr/bin/env python3
"""Fix the one remaining compilation error: stderr_handle moved into async task but still referenced."""

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    lines = f.readlines()

# Find the exact line with the error
target_line = None
for i, line in enumerate(lines):
    if 'if let Some(mut stderr_reader) = stderr_handle' in line:
        target_line = i
        print(f"Found at line {i+1}: {line.rstrip()[:100]}")

if target_line is not None:
    # Show context
    for j in range(max(0, target_line - 2), min(len(lines), target_line + 5)):
        print(f"  {j+1:5d}: {lines[j].rstrip()[:120]}")
    
    # Replace the block: from "let mut err_bytes" to "return Err(err.into_owned())"
    # Find the start (let mut err_bytes) and end (return Err)
    start = target_line
    while start > 0 and 'let mut err_bytes' not in lines[start]:
        start -= 1
    
    end = target_line
    while end < len(lines) and 'return Err(err.into_owned())' not in lines[end]:
        end += 1
    end += 1  # include the return line
    
    print(f"\nReplacing lines {start+1}-{end}")
    for j in range(start, end):
        print(f"  OLD {j+1}: {lines[j].rstrip()[:120]}")
    
    # Build replacement
    replacement = [
        '                    let stderr_bytes = stderr_task.await.unwrap_or_default();\n',
        '                    let err = String::from_utf8_lossy(&stderr_bytes);\n',
        '                    let stdout_bytes = stdout_task.await.unwrap_or_default();\n',
        '                    let stdout_str = String::from_utf8_lossy(&stdout_bytes);\n',
        '                    log::error!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr: {}", status.code(), err);\n',
        '                    if !stdout_str.trim().is_empty() {\n',
        '                        log::info!("[FFMPEG_STDOUT] {}", stdout_str.lines().take(10).collect::<Vec<_>>().join("\\n"));\n',
        '                    }\n',
        '                    return Err(err.into_owned());\n',
    ]
    
    lines = lines[:start] + replacement + lines[end:]
    
    with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
        f.writelines(lines)
    
    print(f"\nFIXED: Replaced {end - start} lines with {len(replacement)} lines")
else:
    print("ERROR: Could not find stderr_handle reference")
