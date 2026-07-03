use crate::types::{FolderForMerge, PartitionMethod, SectionBoundary, SectionPlan};

const MIN_SECTIONS: u32 = 2;
const MAX_SECTIONS: u32 = 50;
const BALANCE_THRESHOLD: f64 = 1.5;

pub fn compute_section_preview(
    folders: Vec<FolderForMerge>,
    method: PartitionMethod,
    name_template: String,
) -> Result<Vec<SectionPlan>, String> {
    let folders: Vec<_> = folders.into_iter().filter(|f| f.include_in_sections).collect();
    
    if folders.is_empty() {
        return Err("No folders to merge".into());
    }
    
    let total_duration: f64 = folders.iter().map(|f| f.duration_secs).sum();
    let _total_videos: usize = folders.iter().map(|f| f.video_count).sum();
    let _total_size: u64 = folders.iter().map(|f| f.size_bytes).sum();
    
    let boundaries = match method {
        PartitionMethod::SectionCount(n) => partition_by_duration(&folders, n, total_duration)?,
        PartitionMethod::MaxDurationSecs(max_dur) => partition_by_max_duration(&folders, max_dur)?,
        PartitionMethod::MaxSizeBytes(max_size) => partition_by_max_size(&folders, max_size)?,
    };
    
    let section_count = boundaries.len() as u32;
    let plans: Vec<SectionPlan> = boundaries
        .into_iter()
        .enumerate()
        .map(|(idx, boundary)| {
            let output_name = resolve_section_name(&name_template, idx as u32, section_count, boundary.duration_secs);
            SectionPlan {
                section_index: idx as u32,
                boundary,
                output_name,
            }
        })
        .collect();
    
    Ok(plans)
}

fn partition_by_duration(folders: &[FolderForMerge], target_sections: u32, total_duration: f64) -> Result<Vec<SectionBoundary>, String> {
    let n = target_sections.clamp(MIN_SECTIONS, MAX_SECTIONS) as usize;
    let target_per_section = total_duration / n as f64;
    
    let mut sections: Vec<SectionBoundary> = Vec::new();
    let mut current = SectionBoundary {
        section_index: 0,
        folder_start_idx: 0,
        folder_end_idx: 0,
        video_count: 0,
        duration_secs: 0.0,
        estimated_size_bytes: 0,
        folder_names: Vec::new(),
    };
    
    for (idx, folder) in folders.iter().enumerate() {
        if current.duration_secs > 0.0 
            && current.duration_secs + folder.duration_secs > target_per_section * BALANCE_THRESHOLD 
            && current.video_count > 0 
            && sections.len() < n - 1 {
            sections.push(current);
            current = SectionBoundary {
                section_index: sections.len() as u32,
                folder_start_idx: idx,
                folder_end_idx: idx + 1,
                video_count: folder.video_count,
                duration_secs: folder.duration_secs,
                estimated_size_bytes: folder.size_bytes,
                folder_names: vec![folder.name.clone()],
            };
        } else {
            current.folder_end_idx = idx + 1;
            current.video_count += folder.video_count;
            current.duration_secs += folder.duration_secs;
            current.estimated_size_bytes += folder.size_bytes;
            if !current.folder_names.contains(&folder.name) {
                current.folder_names.push(folder.name.clone());
            }
        }
    }
    
    if current.video_count > 0 {
        sections.push(current);
    }
    
    if sections.len() > n {
        balance_sections(&mut sections, n);
    }
    
    for (i, section) in sections.iter_mut().enumerate() {
        section.section_index = i as u32;
    }
    
    Ok(sections)
}

fn partition_by_max_duration(folders: &[FolderForMerge], max_duration_secs: f64) -> Result<Vec<SectionBoundary>, String> {
    if max_duration_secs <= 0.0 {
        return Err("Max duration must be positive".into());
    }
    
    let mut sections: Vec<SectionBoundary> = Vec::new();
    let mut current = SectionBoundary {
        section_index: 0,
        folder_start_idx: 0,
        folder_end_idx: 0,
        video_count: 0,
        duration_secs: 0.0,
        estimated_size_bytes: 0,
        folder_names: Vec::new(),
    };
    
    for (idx, folder) in folders.iter().enumerate() {
        if current.duration_secs > 0.0 
            && current.duration_secs + folder.duration_secs > max_duration_secs 
            && current.video_count > 0 {
            sections.push(current);
            current = SectionBoundary {
                section_index: sections.len() as u32,
                folder_start_idx: idx,
                folder_end_idx: idx + 1,
                video_count: folder.video_count,
                duration_secs: folder.duration_secs,
                estimated_size_bytes: folder.size_bytes,
                folder_names: vec![folder.name.clone()],
            };
        } else {
            current.folder_end_idx = idx + 1;
            current.video_count += folder.video_count;
            current.duration_secs += folder.duration_secs;
            current.estimated_size_bytes += folder.size_bytes;
            if !current.folder_names.contains(&folder.name) {
                current.folder_names.push(folder.name.clone());
            }
        }
    }
    
    if current.video_count > 0 {
        sections.push(current);
    }
    
    for (i, section) in sections.iter_mut().enumerate() {
        section.section_index = i as u32;
    }
    
    Ok(sections)
}

fn partition_by_max_size(folders: &[FolderForMerge], max_size_bytes: u64) -> Result<Vec<SectionBoundary>, String> {
    if max_size_bytes == 0 {
        return Err("Max size must be positive".into());
    }
    
    let mut sections: Vec<SectionBoundary> = Vec::new();
    let mut current = SectionBoundary {
        section_index: 0,
        folder_start_idx: 0,
        folder_end_idx: 0,
        video_count: 0,
        duration_secs: 0.0,
        estimated_size_bytes: 0,
        folder_names: Vec::new(),
    };
    
    for (idx, folder) in folders.iter().enumerate() {
        if current.estimated_size_bytes > 0 
            && current.estimated_size_bytes + folder.size_bytes > max_size_bytes 
            && current.video_count > 0 {
            sections.push(current);
            current = SectionBoundary {
                section_index: sections.len() as u32,
                folder_start_idx: idx,
                folder_end_idx: idx + 1,
                video_count: folder.video_count,
                duration_secs: folder.duration_secs,
                estimated_size_bytes: folder.size_bytes,
                folder_names: vec![folder.name.clone()],
            };
        } else {
            current.folder_end_idx = idx + 1;
            current.video_count += folder.video_count;
            current.duration_secs += folder.duration_secs;
            current.estimated_size_bytes += folder.size_bytes;
            if !current.folder_names.contains(&folder.name) {
                current.folder_names.push(folder.name.clone());
            }
        }
    }
    
    if current.video_count > 0 {
        sections.push(current);
    }
    
    for (i, section) in sections.iter_mut().enumerate() {
        section.section_index = i as u32;
    }
    
    Ok(sections)
}

fn balance_sections(sections: &mut Vec<SectionBoundary>, target: usize) {
    if sections.len() <= target {
        return;
    }
    
    let target_f = target as f64;
    let ideal_size = sections.iter().map(|s| s.duration_secs).sum::<f64>() / target_f;
    
    let mut merged: Vec<SectionBoundary> = Vec::new();
    let mut current = sections.remove(0);
    
    for section in sections.drain(..) {
        if current.duration_secs >= ideal_size && merged.len() < target - 1 {
            merged.push(current);
            current = section;
        } else {
            current.folder_end_idx = section.folder_end_idx;
            current.video_count += section.video_count;
            current.duration_secs += section.duration_secs;
            current.estimated_size_bytes += section.estimated_size_bytes;
            for name in section.folder_names {
                if !current.folder_names.contains(&name) {
                    current.folder_names.push(name);
                }
            }
        }
    }
    
    if current.video_count > 0 {
        merged.push(current);
    }
    
    *sections = merged;
}

fn resolve_section_name(template: &str, index: u32, total: u32, duration_secs: f64) -> String {
    let index_str = format!("{:02}", index + 1);
    let total_str = total.to_string();
    let duration_str = format_duration(duration_secs);
    let date_str = chrono::Utc::now().format("%Y%m%d").to_string();
    
    template
        .replace("{N}", &index_str)
        .replace("{TOTAL}", &total_str)
        .replace("{DURATION}", &duration_str)
        .replace("{DATE}", &date_str)
}

fn format_duration(secs: f64) -> String {
    let hours = (secs / 3600.0).floor() as u32;
    let minutes = ((secs % 3600.0) / 60.0).floor() as u32;
    format!("{:02}h{:02}m", hours, minutes)
}

pub fn resolve_file_paths_for_section(
    folders: &[FolderForMerge],
    boundary: &SectionBoundary,
) -> Vec<String> {
    folders[boundary.folder_start_idx..boundary.folder_end_idx]
        .iter()
        .flat_map(|f| f.file_paths.clone())
        .collect()
}

pub fn compute_playlist_hash(folders: &[FolderForMerge]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    
    for folder in folders {
        folder.name.hash(&mut hasher);
        folder.path.hash(&mut hasher);
        folder.file_paths.len().hash(&mut hasher);
        folder.size_bytes.hash(&mut hasher);
    }
    
    format!("{:016x}", hasher.finish())
}