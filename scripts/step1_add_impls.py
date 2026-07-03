#!/usr/bin/env python3
"""Step 1b: Add convenience methods for FileState and PipelineReport."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# Find the closing brace of PipelineReport struct
# Look for "pub total_revalidation_duration_secs: f64," followed by "}"
marker = 'pub total_revalidation_duration_secs: f64,\n}'
if marker not in content:
    print("ERROR: Could not find PipelineReport closing brace")
    exit(1)

idx = content.index(marker) + len(marker)

impls = """

impl FileState {
    /// Returns true if this file is healthy and needs no repair.
    pub fn is_healthy(&self) -> bool {
        matches!(self.disposition, FileDisposition::Healthy)
    }

    /// Returns true if this file has damage (repairable or not).
    pub fn is_damaged(&self) -> bool {
        matches!(self.disposition, FileDisposition::Repairable(_) | FileDisposition::Unrepairable(_))
    }

    /// Returns true if this file was quarantined (excluded from merge).
    pub fn is_quarantined(&self) -> bool {
        self.final_path.is_empty()
    }

    /// Returns true if this file needs repair in Phase 2.
    pub fn needs_repair(&self) -> bool {
        matches!(self.disposition, FileDisposition::Repairable(_))
    }

    /// Returns true if this file needs compatibility normalization.
    pub fn needs_normalization(&self) -> bool {
        matches!(self.disposition, FileDisposition::NeedsNormalization)
    }

    /// Returns true if this file was successfully repaired in Phase 2.
    pub fn was_repaired(&self) -> bool {
        self.repair_status == RepairStatus::Succeeded
    }

    /// Returns true if this file will participate in the merge.
    pub fn will_merge(&self) -> bool {
        !self.final_path.is_empty()
    }
}

impl PipelineReport {
    /// Returns an iterator over files that are healthy.
    pub fn healthy_files(&self) -> impl Iterator<Item = &FileState> {
        self.file_states.iter().filter(|s| s.is_healthy())
    }

    /// Returns an iterator over files that were repaired.
    pub fn repaired_files(&self) -> impl Iterator<Item = &FileState> {
        self.file_states.iter().filter(|s| s.was_repaired())
    }

    /// Returns an iterator over files that are quarantined.
    pub fn quarantined_files(&self) -> impl Iterator<Item = &FileState> {
        self.file_states.iter().filter(|s| s.is_quarantined())
    }

    /// Returns an iterator over files that will participate in the merge.
    pub fn merge_files(&self) -> impl Iterator<Item = &FileState> {
        self.file_states.iter().filter(|s| s.will_merge())
    }

    /// Returns an iterator over files that need normalization.
    pub fn needs_normalization_files(&self) -> impl Iterator<Item = &FileState> {
        self.file_states.iter().filter(|s| s.needs_normalization())
    }
}
"""

content = content[:idx] + impls + content[idx:]

with open(path, 'w', encoding='utf-8') as f:
    f.write(content)

print("Added impl blocks for FileState and PipelineReport")

# Verify
with open(path, 'r', encoding='utf-8') as f:
    verify = f.read()

checks = [
    'impl FileState',
    'fn is_healthy',
    'fn is_damaged',
    'fn is_quarantined',
    'fn needs_repair',
    'fn will_merge',
    'impl PipelineReport',
    'fn healthy_files',
    'fn repaired_files',
    'fn quarantined_files',
    'fn merge_files',
]
for c in checks:
    print(f"  [{'PASS' if c in verify else 'FAIL'}] {c}")
