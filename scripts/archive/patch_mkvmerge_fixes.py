#!/usr/bin/env python3
"""Fix mkvmerge.rs: remove redundant write_raw calls and fix stderr deadlock."""

with open('src-tauri/src/ffmpeg/mkvmerge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

changes = 0

# 1. Remove redundant write_raw for MKVMERGE_CMD
idx = content.find('crate::logger::write_raw(&format!("[MKVMERGE_CMD]')
if idx >= 0:
    # Find the end of this line (includes \r\n)
    line_end = content.find('\n', idx) + 1
    content = content[:idx] + content[line_end:]
    changes += 1
    print('Removed redundant MKVMERGE_CMD write_raw')
else:
    print('MKVMERGE_CMD write_raw not found')

# 2. Remove redundant write_raw for MKVMERGE_STDERR
idx = content.find('crate::logger::write_raw(&format!("[MKVMERGE_STDERR]')
if idx >= 0:
    line_end = content.find('\n', idx) + 1
    content = content[:idx] + content[line_end:]
    changes += 1
    print('Removed redundant MKVMERGE_STDERR write_raw')
else:
    print('MKVMERGE_STDERR write_raw not found')

# 3. Remove redundant write_raw for MKVMERGE_EXIT
idx = content.find('crate::logger::write_raw(&format!("[MKVMERGE_EXIT] Exit code:')
if idx >= 0:
    line_end = content.find('\n', idx) + 1
    content = content[:idx] + content[line_end:]
    changes += 1
    print('Removed redundant MKVMERGE_EXIT write_raw')
else:
    print('MKVMERGE_EXIT write_raw not found')

# 4. Fix stderr deadlock: move stderr read to background thread before wait
# Find the block: "// Log stderr from mkvmerge" ... "let status = child.wait()"
stderr_start = content.find('// Log stderr from mkvmerge')
if stderr_start < 0:
    stderr_start = content.find('// Drain stderr')  # Already patched?
status_line = content.find('let status = child.wait()', stderr_start) if stderr_start >= 0 else -1

if stderr_start >= 0 and status_line >= 0:
    # Find the block boundaries
    block_start = content.rfind('\n', 0, stderr_start) + 1
    block_end = content.find('\n', status_line)
    
    old_block = content[block_start:block_end]
    print(f'Found stderr block ({len(old_block)} chars)')
    
    new_block = """    // Drain stderr in a background thread to prevent pipe buffer full deadlock.
    // If stderr fills up while we read stdout, mkvmerge blocks -> deadlock.
    let stderr_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let mut reader = std::io::BufReader::new(stderr);
        let _ = reader.read_to_end(&mut buf);
        buf
    });

    let status = child.wait().map_err(|e| anyhow!("mkvmerge wait failed: {}", e))?;
    let exit_code = status.code().unwrap_or(-1);
    log::info!("[MKVMERGE_EXIT] Exit code: {}", exit_code);

    // Collect stderr from background thread
    let stderr_bytes = stderr_handle.join().unwrap_or_default();
    let stderr_str = String::from_utf8_lossy(&stderr_bytes);
    if !stderr_str.trim().is_empty() {
        log::warn!("[MKVMERGE_STDERR] {}", stderr_str.trim());
    }"""
    
    content = content[:block_start] + new_block + content[block_end:]
    changes += 1
    print('Fixed stderr deadlock with background thread')
else:
    print(f'stderr block not found (start={stderr_start}, status={status_line})')

with open('src-tauri/src/ffmpeg/mkvmerge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print(f'\nmkvmerge.rs: {changes} fixes applied')
