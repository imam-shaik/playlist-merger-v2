use crate::types::RepeatConfig;

/// Maximum allowed expanded file count to prevent memory explosion
const MAX_EXPANDED_FILES: usize = 10_000;

/// Result of expanding a playlist by repeat configuration
#[derive(Debug, Clone)]
pub struct ExpandedPlaylist {
    /// Expanded file paths (physical duplication of references)
    pub files: Vec<String>,
    /// Durations matching expanded files
    pub durations: Vec<f64>,
    /// Names matching expanded files
    pub names: Vec<String>,
    /// Mapping from expanded index to original index
    /// Each entry is (repeat_cycle, original_index)
    pub index_mapping: Vec<(u32, usize)>,
    /// Original file count before expansion
    pub original_count: usize,
    /// Effective repeat count applied
    pub repeat_count: u32,
    /// Total duration after expansion
    pub total_duration: f64,
    /// Whether to insert boundary cards at repeat cycle starts
    pub insert_boundary_cards: bool,
    /// Template for boundary card labels. {n} is replaced with cycle number.
    pub boundary_card_template: String,
}

impl ExpandedPlaylist {
    /// Returns the repeat cycle number for a given expanded index.
    /// Returns 0 if index_mapping is empty.
    pub fn repeat_cycle(&self, index: usize) -> u32 {
        self.index_mapping
            .get(index)
            .map(|(cycle, _)| *cycle)
            .unwrap_or(0)
    }

    /// Returns true if a boundary card should be inserted at the given expanded index.
    /// A boundary card is inserted at the start of each repeat cycle (index 0, original_count, 2*original_count, etc.)
    pub fn should_insert_boundary_at(&self, index: usize) -> bool {
        if !self.insert_boundary_cards || self.original_count == 0 {
            return false;
        }
        index.is_multiple_of(self.original_count)
    }

/// Returns the boundary card label for a given cycle number.
    /// Replaces {n} with the cycle number (1-indexed: cycle 1 → "1", cycle 2 → "2", etc.)
    pub fn get_boundary_label(&self, cycle: u32) -> String {
        self.boundary_card_template.replace("{n}", &cycle.to_string())
    }
}

/// Expand a playlist based on repeat configuration.
///
/// Returns `None` if repeat is disabled or无效.
/// Returns `Some(ExpandedPlaylist)` with physically duplicated file references.
pub fn expand_repeat(
    config: &RepeatConfig,
    files: &[String],
    durations: &[f64],
    names: &[String],
) -> Option<ExpandedPlaylist> {
    if !config.enabled || files.is_empty() {
        return None;
    }

    let original_count = files.len();
    let original_duration: f64 = durations.iter().sum();

    if original_duration <= 0.0 {
        log::warn!("[Repeat] Original duration is zero, cannot calculate repeat count");
        return None;
    }

    // Calculate effective repeat count
    let mut repeat_count: u32 = 0;

    if config.by_count && config.until_duration {
        // Both modes: use the MAX (ensure BOTH constraints are met)
        // by_count sets a ceiling, until_duration sets a floor
        // Must satisfy both, so use whichever requires more repeats
        let count_from_count = config.repeat_count;
        let count_from_duration = (config.target_duration_seconds / original_duration).ceil() as u32;
        repeat_count = count_from_count.max(count_from_duration);
    } else if config.by_count {
        repeat_count = config.repeat_count;
    } else if config.until_duration {
        repeat_count = (config.target_duration_seconds / original_duration).ceil() as u32;
    }

    if repeat_count <= 1 {
        // No expansion needed
        return None;
    }

    // Check expanded file count guard
    let expanded_count = original_count * repeat_count as usize;
    if expanded_count > MAX_EXPANDED_FILES {
        log::warn!(
            "[Repeat] Expanded file count {} exceeds maximum {} — capping repeat count",
            expanded_count, MAX_EXPANDED_FILES
        );
        repeat_count = (MAX_EXPANDED_FILES / original_count) as u32;
        if repeat_count <= 1 {
            log::warn!("[Repeat] Cannot expand: too many original files for limit");
            return None;
        }
    }

    // Build expanded lists by physical duplication
    let mut expanded_files = Vec::with_capacity(expanded_count);
    let mut expanded_durations = Vec::with_capacity(expanded_count);
    let mut expanded_names = Vec::with_capacity(expanded_count);
    let mut index_mapping = Vec::with_capacity(expanded_count);

    for cycle in 0..repeat_count {
        for (orig_idx, file) in files.iter().enumerate() {
            expanded_files.push(file.clone());
            expanded_durations.push(durations[orig_idx]);
            expanded_names.push(names[orig_idx].clone());
            index_mapping.push((cycle, orig_idx));
        }
    }

    let total_duration = expanded_durations.iter().sum();

    log::info!(
        "[Repeat] Expanded {} files × {} = {} files (original_duration={:.1}s, total_duration={:.1}s)",
        original_count, repeat_count, expanded_files.len(), original_duration, total_duration
    );

    Some(ExpandedPlaylist {
        files: expanded_files,
        durations: expanded_durations,
        names: expanded_names,
        index_mapping,
        original_count,
        repeat_count,
        total_duration,
        insert_boundary_cards: config.insert_boundary_cards,
        boundary_card_template: config.boundary_card_template.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(by_count: bool, count: u32, until_duration: bool, target_secs: f64) -> RepeatConfig {
        RepeatConfig {
            enabled: true,
            by_count,
            repeat_count: count,
            until_duration,
            target_duration_seconds: target_secs,
            insert_boundary_cards: false,
            boundary_card_template: "🔁 Repeat {n}".to_string(),
        }
    }

    #[test]
    fn test_single_video_by_count() {
        let config = test_config(true, 3, false, 0.0);
        let files = vec!["a.mp4".to_string()];
        let durations = vec![300.0];
        let names = vec!["a".to_string()];

        let result = expand_repeat(&config, &files, &durations, &names).unwrap();
        assert_eq!(result.files.len(), 3);
        assert_eq!(result.repeat_count, 3);
        assert!((result.total_duration - 900.0).abs() < 0.01);
    }

    #[test]
    fn test_playlist_by_count() {
        let config = test_config(true, 2, false, 0.0);
        let files = vec!["a.mp4".to_string(), "b.mp4".to_string(), "c.mp4".to_string()];
        let durations = vec![100.0, 200.0, 300.0];
        let names = vec!["a".to_string(), "b".to_string(), "c".to_string()];

        let result = expand_repeat(&config, &files, &durations, &names).unwrap();
        assert_eq!(result.files.len(), 6);
        assert_eq!(result.files[0], "a.mp4");
        assert_eq!(result.files[3], "a.mp4");
        assert!((result.total_duration - 1200.0).abs() < 0.01);
    }

    #[test]
    fn test_until_duration() {
        // 5 min video, target 2 hours = 120 min = 24 repeats
        let config = test_config(false, 0, true, 7200.0);
        let files = vec!["a.mp4".to_string()];
        let durations = vec![300.0];
        let names = vec!["a".to_string()];

        let result = expand_repeat(&config, &files, &durations, &names).unwrap();
        assert_eq!(result.repeat_count, 24);
        assert_eq!(result.files.len(), 24);
    }

    #[test]
    fn test_both_modes_max() {
        // by_count=3, until_duration needs 5 repeats → max = 5
        let config = test_config(true, 3, true, 1500.0);
        let files = vec!["a.mp4".to_string()];
        let durations = vec![300.0];
        let names = vec!["a".to_string()];

        let result = expand_repeat(&config, &files, &durations, &names).unwrap();
        assert_eq!(result.repeat_count, 5);
    }

    #[test]
    fn test_disabled() {
        let config = RepeatConfig {
            enabled: false,
            by_count: true,
            repeat_count: 5,
            until_duration: false,
            target_duration_seconds: 0.0,
            insert_boundary_cards: false,
            boundary_card_template: "🔁 Repeat {n}".to_string(),
        };
        let files = vec!["a.mp4".to_string()];
        let durations = vec![300.0];
        let names = vec!["a".to_string()];

        assert!(expand_repeat(&config, &files, &durations, &names).is_none());
    }

    #[test]
    fn test_count_one() {
        let config = test_config(true, 1, false, 0.0);
        let files = vec!["a.mp4".to_string()];
        let durations = vec![300.0];
        let names = vec!["a".to_string()];

        // count=1 means no expansion
        assert!(expand_repeat(&config, &files, &durations, &names).is_none());
    }

    #[test]
    fn test_index_mapping() {
        let config = test_config(true, 2, false, 0.0);
        let files = vec!["a.mp4".to_string(), "b.mp4".to_string()];
        let durations = vec![100.0, 200.0];
        let names = vec!["a".to_string(), "b".to_string()];

        let result = expand_repeat(&config, &files, &durations, &names).unwrap();
        assert_eq!(result.index_mapping[0], (0, 0)); // cycle 0, idx 0
        assert_eq!(result.index_mapping[1], (0, 1)); // cycle 0, idx 1
        assert_eq!(result.index_mapping[2], (1, 0)); // cycle 1, idx 0
        assert_eq!(result.index_mapping[3], (1, 1)); // cycle 1, idx 1
    }

    #[test]
    fn test_boundary_card_api() {
        let config = RepeatConfig {
            enabled: true,
            by_count: true,
            repeat_count: 3,
            until_duration: false,
            target_duration_seconds: 0.0,
            insert_boundary_cards: true,
            boundary_card_template: "🔁 Repeat {n}".to_string(),
        };
        let files = vec!["a.mp4".to_string(), "b.mp4".to_string()];
        let durations = vec![100.0, 200.0];
        let names = vec!["a".to_string(), "b".to_string()];

        let result = expand_repeat(&config, &files, &durations, &names).unwrap();
        assert_eq!(result.repeat_cycle(0), 0);
        assert_eq!(result.repeat_cycle(1), 0);
        assert_eq!(result.repeat_cycle(2), 1);
        assert_eq!(result.repeat_cycle(3), 1);
        assert_eq!(result.repeat_cycle(4), 2);
        assert_eq!(result.repeat_cycle(5), 2);

        assert!(result.should_insert_boundary_at(0)); // cycle 0 start
        assert!(!result.should_insert_boundary_at(1)); // cycle 0
        assert!(result.should_insert_boundary_at(2)); // cycle 1 start
        assert!(!result.should_insert_boundary_at(3)); // cycle 1
        assert!(result.should_insert_boundary_at(4)); // cycle 2 start
        assert!(!result.should_insert_boundary_at(5)); // cycle 2

        assert_eq!(result.get_boundary_label(1), "🔁 Repeat 1");
        assert_eq!(result.get_boundary_label(2), "🔁 Repeat 2");
        assert_eq!(result.get_boundary_label(3), "🔁 Repeat 3");
    }

    #[test]
    fn test_boundary_cards_disabled() {
        let config = RepeatConfig {
            enabled: true,
            by_count: true,
            repeat_count: 2,
            until_duration: false,
            target_duration_seconds: 0.0,
            insert_boundary_cards: false,
            boundary_card_template: "🔁 Repeat {n}".to_string(),
        };
        let files = vec!["a.mp4".to_string()];
        let durations = vec![100.0];
        let names = vec!["a".to_string()];

        let result = expand_repeat(&config, &files, &durations, &names).unwrap();
        assert!(!result.should_insert_boundary_at(0));
        assert!(!result.should_insert_boundary_at(1));
    }
}
