"""
Fix index mismatch bug in merge.rs media validation integration.
Apply path updates BEFORE removing quarantined files, not after.
"""

filepath = 'src-tauri/src/commands/merge.rs'

with open(filepath, 'r', encoding='utf-8') as f:
    content = f.read()

has_crlf = '\r\n' in content

# The old ordering: remove quarantined files first, THEN apply path updates
# The new ordering: apply path updates FIRST, THEN remove quarantined files

# Find the block that needs reordering
old_block = '''    // Apply validation results: update working files
    let (updated_files, removed_indices) = apply_validation_results(&working_input_files, &media_report);

    if !removed_indices.is_empty() {
        log::warn!("[MEDIA_VALIDATION] {} files quarantined (removed from pipeline)", removed_indices.len());
        // Rebuild working arrays excluding quarantined files
        let remove_set: std::collections::HashSet<usize> = removed_indices.into_iter().collect();
        let mut new_files = Vec::new();
        let mut new_durations = Vec::new();
        let mut new_names = Vec::new();
        for (i, file) in working_input_files.iter().enumerate() {
            if !remove_set.contains(&i) {
                new_files.push(file.clone());
                new_durations.push(working_input_durations[i]);
                new_names.push(working_input_names[i].clone());
            }
        }
        working_input_files = new_files;
        working_input_durations = new_durations;
        working_input_names = new_names;
        working_total_duration = working_input_durations.iter().sum();
    }

    // Apply path updates (repaired files may have new paths)
    let mut reparsed_count = 0usize;
    for (i, file) in working_input_files.iter_mut().enumerate() {
        if i < updated_files.len() && updated_files[i] != *file {
            *file = updated_files[i].clone();
            reparsed_count += 1;
        }
    }
    if reparsed_count > 0 {
        log::info!("[MEDIA_VALIDATION] Updated {} file paths from repaired paths", reparsed_count);
    }'''

new_block = '''    // Apply validation results: update working files
    let (updated_files, removed_indices) = apply_validation_results(&working_input_files, &media_report);

    // STEP 1: Apply path updates FIRST (using original indices before any removal)
    let mut reparsed_count = 0usize;
    for (i, file) in working_input_files.iter_mut().enumerate() {
        if i < updated_files.len() && updated_files[i] != *file {
            *file = updated_files[i].clone();
            reparsed_count += 1;
        }
    }
    if reparsed_count > 0 {
        log::info!("[MEDIA_VALIDATION] Updated {} file paths from repaired paths", reparsed_count);
    }

    // STEP 2: Remove quarantined files (index-safe because path updates already applied)
    if !removed_indices.is_empty() {
        log::warn!("[MEDIA_VALIDATION] {} files quarantined (removed from pipeline)", removed_indices.len());
        let remove_set: std::collections::HashSet<usize> = removed_indices.into_iter().collect();
        let mut new_files = Vec::new();
        let mut new_durations = Vec::new();
        let mut new_names = Vec::new();
        for (i, file) in working_input_files.iter().enumerate() {
            if !remove_set.contains(&i) {
                new_files.push(file.clone());
                new_durations.push(working_input_durations[i]);
                new_names.push(working_input_names[i].clone());
            }
        }
        working_input_files = new_files;
        working_input_durations = new_durations;
        working_input_names = new_names;
        working_total_duration = working_input_durations.iter().sum();
    }'''

if old_block in content:
    content = content.replace(old_block, new_block, 1)
    print('Fix: Reordered path updates before quarantine removal to fix index bug')
else:
    print('Fix FAILED: Block not found')
    # Find the apply_validation_results call
    idx = content.find('apply_validation_results(&working_input_files, &media_report)')
    if idx >= 0:
        print(f'  Found apply_validation_results at {idx}')
        start = max(0, idx - 50)
        print(f'  Context: {repr(content[start:idx+400])}')
    else:
        print('  apply_validation_results not found')

# Write the file back
with open(filepath, 'w', encoding='utf-8', newline='\r\n' if has_crlf else '\n') as f:
    f.write(content)

print(f'\nWritten to {filepath}')
print('Done!')
