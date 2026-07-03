
pub use crate::ffmpeg::normalization::MergePlan;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionPlan {
    pub backend: BackendPlan,
    pub merge_mode: String,
    pub input_files: Vec<InputFilePlan>,
    pub output_path: String,
    pub ffmpeg: FfmpegPlan,
    pub mkvmerge: Option<MkvMergePlan>,
    pub concat_list: ConcatListPlan,
    pub subtitle_plan: SubtitlePlan,
    pub chapter_plan: ChapterPlan,
    pub metadata_plan: MetadataPlan,
    pub normalization_plan: NormalizationPlan,
    pub execution_hash: String,
    pub semantic_hash: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct BackendPlan {
    pub backend_type: String,
    pub ffmpeg_path: String,
    pub mkvmerge_path: Option<String>,
    pub ffprobe_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct InputFilePlan {
    pub index: usize,
    pub path: String,
    pub normalized_path: String,
    pub duration_seconds: f64,
    pub has_video: bool,
    pub has_audio: bool,
    pub has_subtitle: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FfmpegPlan {
    pub executable: String,
    pub executable_normalized: String,
    pub arguments: Vec<String>,
    pub argument_hash: String,
    pub arguments_normalized: Vec<String>,
    pub semantic_argument_hash: String,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_crf: Option<u32>,
    pub video_preset: Option<String>,
    pub audio_bitrate: Option<String>,
    pub target_resolution: Option<String>,
    pub target_fps: Option<String>,
    pub hw_accel: Option<String>,
    pub subtitle_mode: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MkvMergePlan {
    pub executable: String,
    pub arguments: Vec<String>,
    pub argument_hash: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ConcatListPlan {
    pub file_path: String,
    pub file_path_normalized: String,
    pub content_hash: String,
    pub entry_count: usize,
    pub entries: Vec<ConcatEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ConcatEntry {
    pub file_path: String,
    pub file_path_normalized: String,
    pub duration_seconds: f64,
    pub start_seconds: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SubtitlePlan {
    pub mode: String,
    pub subtitle_list_path: Option<String>,
    pub subtitle_files: Vec<SubtitleFilePlan>,
    pub burn_subtitle_path: Option<String>,
    pub merged_srt_path: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SubtitleFilePlan {
    pub index: usize,
    pub original_path: Option<String>,
    pub extracted_path: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ChapterPlan {
    pub has_chapters: bool,
    pub chapter_file_path: Option<String>,
    pub chapter_count: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MetadataPlan {
    pub has_metadata: bool,
    pub metadata_file_path: Option<String>,
    pub title: Option<String>,
    pub encoder: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NormalizationPlan {
    pub files_requiring_normalization: Vec<NormalizationFilePlan>,
    pub normalized_output_dir: String,
    pub cache_dir: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct NormalizationFilePlan {
    pub index: usize,
    pub original_path: String,
    pub normalized_path: Option<String>,
    pub normalization_type: String,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub target_profile: Option<String>,
}

impl ExecutionPlan {
    pub fn new(
        _merge_plan: &MergePlan,
        merge_config: &crate::ffmpeg::concat::MergeConfig,
        ffmpeg_path: &str,
        ffprobe_path: &str,
        mkvmerge_path: Option<&str>,
        ffmpeg_args: &[String],
        concat_content: &str,
        concat_entries: &[(String, f64)],
    ) -> Self {
        let mut plan = ExecutionPlan {
            backend: BackendPlan {
                backend_type: format!("{:?}", merge_config.mode),
                ffmpeg_path: ffmpeg_path.to_string(),
                mkvmerge_path: mkvmerge_path.map(String::from),
                ffprobe_path: ffprobe_path.to_string(),
            },
            merge_mode: format!("{:?}", merge_config.mode),
            input_files: Self::build_input_files(merge_config),
            output_path: merge_config.output_path.clone(),
            ffmpeg: FfmpegPlan {
                executable: ffmpeg_path.to_string(),
                executable_normalized: normalize_executable(ffmpeg_path),
                arguments: ffmpeg_args.to_vec(),
                argument_hash: hash_strings(ffmpeg_args),
                arguments_normalized: normalize_arguments(ffmpeg_args),
                semantic_argument_hash: hash_strings(&normalize_arguments(ffmpeg_args)),
                video_codec: merge_config.video_codec.clone(),
                audio_codec: merge_config.audio_codec.clone(),
                video_crf: merge_config.video_crf,
                video_preset: merge_config.video_preset.clone(),
                audio_bitrate: merge_config.audio_bitrate.clone(),
                target_resolution: merge_config.target_resolution.clone(),
                target_fps: merge_config.target_fps.clone(),
                hw_accel: merge_config.hw_accel.clone(),
                subtitle_mode: format!("{:?}", merge_config.subtitle_mode),
            },
            mkvmerge: None,
            concat_list: ConcatListPlan {
                file_path: String::new(),
                file_path_normalized: String::from("concat_list.txt"),
                content_hash: hash_string(concat_content),
                entry_count: concat_entries.len(),
                entries: concat_entries.iter().enumerate().map(|(i, (path, dur))| {
                    ConcatEntry {
                        file_path: path.clone(),
                        file_path_normalized: normalize_path(path),
                        duration_seconds: *dur,
                        start_seconds: concat_entries.iter().take(i).map(|(_, d)| d).sum::<f64>(),
                    }
                }).collect(),
            },
            subtitle_plan: SubtitlePlan {
                mode: format!("{:?}", merge_config.subtitle_mode),
                subtitle_list_path: merge_config.subtitle_list_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                subtitle_files: vec![],
                burn_subtitle_path: merge_config.burn_subtitle_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                merged_srt_path: None,
            },
            chapter_plan: ChapterPlan {
                has_chapters: false,
                chapter_file_path: None,
                chapter_count: 0,
            },
            metadata_plan: MetadataPlan {
                has_metadata: false,
                metadata_file_path: None,
                title: None,
                encoder: None,
            },
            normalization_plan: NormalizationPlan {
                files_requiring_normalization: vec![],
                normalized_output_dir: String::new(),
                cache_dir: String::new(),
            },
            execution_hash: String::new(),
            semantic_hash: String::new(),
        };

        plan.execution_hash = plan.compute_hash();
        plan.semantic_hash = plan.compute_semantic_hash();
        plan
    }

    fn build_input_files(config: &crate::ffmpeg::concat::MergeConfig) -> Vec<InputFilePlan> {
        config.input_files.iter().enumerate().map(|(i, path)| {
            let duration = config.input_durations.get(i).copied().unwrap_or(0.0);
            InputFilePlan {
                index: i,
                path: path.clone(),
                normalized_path: normalize_path(path),
                duration_seconds: duration,
                has_video: true,
                has_audio: true,
                has_subtitle: config.subtitle_files.get(i).and_then(|s| s.as_ref()).is_some(),
            }
        }).collect()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| String::new())
    }

    pub fn to_json_compact(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::new())
    }

    pub fn compute_hash(&self) -> String {
        let json = self.to_json();
        sha256_string(&json)
    }

    pub fn compute_semantic_hash(&self) -> String {
        let semantic_plan = SemanticExecutionPlan::from_execution(self);
        serde_json::to_string(&semantic_plan).map(|json| sha256_string(&json)).unwrap_or_else(|_| String::new())
    }

    pub fn equals(&self, other: &ExecutionPlan) -> bool {
        self.backend == other.backend
            && self.merge_mode == other.merge_mode
            && self.input_files == other.input_files
            && self.output_path == other.output_path
            && self.ffmpeg == other.ffmpeg
            && self.concat_list == other.concat_list
            && self.subtitle_plan == other.subtitle_plan
            && self.chapter_plan == other.chapter_plan
            && self.metadata_plan == other.metadata_plan
    }

    pub fn semantic_equals(&self, other: &ExecutionPlan) -> bool {
        self.ffmpeg.arguments_normalized == other.ffmpeg.arguments_normalized
            && self.ffmpeg.video_codec == other.ffmpeg.video_codec
            && self.ffmpeg.audio_codec == other.ffmpeg.audio_codec
            && self.ffmpeg.video_crf == other.ffmpeg.video_crf
            && self.ffmpeg.subtitle_mode == other.ffmpeg.subtitle_mode
            && self.input_files.iter().map(|f| &f.normalized_path).eq(other.input_files.iter().map(|f| &f.normalized_path))
            && self.concat_list.entry_count == other.concat_list.entry_count
            && self.concat_list.entries.iter().map(|e| &e.file_path_normalized).eq(
                other.concat_list.entries.iter().map(|e| &e.file_path_normalized))
    }

    pub fn ffmpeg_argument_hash(&self) -> String {
        self.ffmpeg.argument_hash.clone()
    }

    pub fn semantic_argument_hash(&self) -> String {
        self.ffmpeg.semantic_argument_hash.clone()
    }

    pub fn concat_content_hash(&self) -> String {
        self.concat_list.content_hash.clone()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SemanticExecutionPlan {
    merge_mode: String,
    input_files: Vec<SemanticInputFile>,
    ffmpeg: SemanticFfmpegPlan,
    concat_entry_count: usize,
    subtitle_mode: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SemanticInputFile {
    normalized_path: String,
    duration_seconds: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SemanticFfmpegPlan {
    executable_normalized: String,
    arguments_normalized: Vec<String>,
    semantic_argument_hash: String,
    video_codec: Option<String>,
    audio_codec: Option<String>,
    video_crf: Option<u32>,
    subtitle_mode: String,
}

impl SemanticExecutionPlan {
    fn from_execution(plan: &ExecutionPlan) -> Self {
        SemanticExecutionPlan {
            merge_mode: plan.merge_mode.clone(),
            input_files: plan.input_files.iter().map(|f| SemanticInputFile {
                normalized_path: f.normalized_path.clone(),
                duration_seconds: f.duration_seconds,
            }).collect(),
            ffmpeg: SemanticFfmpegPlan {
                executable_normalized: plan.ffmpeg.executable_normalized.clone(),
                arguments_normalized: plan.ffmpeg.arguments_normalized.clone(),
                semantic_argument_hash: plan.ffmpeg.semantic_argument_hash.clone(),
                video_codec: plan.ffmpeg.video_codec.clone(),
                audio_codec: plan.ffmpeg.audio_codec.clone(),
                video_crf: plan.ffmpeg.video_crf,
                subtitle_mode: plan.ffmpeg.subtitle_mode.clone(),
            },
            concat_entry_count: plan.concat_list.entry_count,
            subtitle_mode: plan.subtitle_plan.mode.clone(),
        }
    }
}

fn normalize_path(path: &str) -> String {
    let p = std::path::Path::new(path);
    p.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string())
}

fn normalize_executable(path: &str) -> String {
    let p = std::path::Path::new(path);
    p.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string())
}

fn normalize_arguments(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|a| {
            let p = std::path::Path::new(a);
            if p.exists() {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| a.to_string())
            } else {
                a.to_string()
            }
        })
        .collect()
}

impl FfmpegPlan {
    pub fn arguments_summary(&self) -> String {
        format!("{} {} (hash: {})",
            self.executable,
            self.arguments.join(" "),
            self.argument_hash.chars().take(12).collect::<String>())
    }

    pub fn semantic_summary(&self) -> String {
        format!("executable={} args={} codec={}/{} crf={:?} sub={} (sem_hash: {})",
            self.executable_normalized,
            self.arguments_normalized.len(),
            self.video_codec.as_deref().unwrap_or("copy"),
            self.audio_codec.as_deref().unwrap_or("copy"),
            self.video_crf,
            self.subtitle_mode,
            self.semantic_argument_hash.chars().take(12).collect::<String>())
    }
}

fn hash_string(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn hash_strings(strings: &[String]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for s in strings {
        s.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn sha256_string(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:032x}", hasher.finish())
}

pub struct ExecutionCertReport {
    pub total_runs: usize,
    pub passed: bool,
    pub semantic_passed: bool,
    pub execution_hashes: Vec<String>,
    pub semantic_hashes: Vec<String>,
    pub comparisons: Vec<ExecutionComparison>,
    pub total_duration_ms: u128,
}

pub struct ExecutionComparison {
    pub run_a: usize,
    pub run_b: usize,
    pub plans_identical: bool,
    pub semantic_identical: bool,
    pub ffmpeg_args_identical: bool,
    pub ffmpeg_semantic_identical: bool,
    pub concat_identical: bool,
    pub first_difference: Option<String>,
}

pub fn certify_execution_idempotency<F>(
    playlist_name: &str,
    runs: usize,
    build_execution_plan: F,
) -> ExecutionCertReport
where
    F: Fn(usize) -> Option<ExecutionPlan>,
{
    use std::time::Instant;

    let start = Instant::now();
    let mut execution_hashes = Vec::new();
    let mut semantic_hashes = Vec::new();
    let mut comparisons = Vec::new();

    log::info!("[CERT:EXECUTION] ═══════════════════════════════════════════════════════════");
    log::info!("[CERT:EXECUTION] EXECUTION IDEMPOTENCY CERTIFICATION");
    log::info!("[CERT:EXECUTION] Playlist: {}", playlist_name);
    log::info!("[CERT:EXECUTION] Runs: {}", runs);
    log::info!("[CERT:EXECUTION] ═══════════════════════════════════════════════════════════");

    for run_id in 0..runs {
        let run_start = Instant::now();
        log::info!("[CERT:EXECUTION] Run {}/{} starting...", run_id + 1, runs);

        let plan = match build_execution_plan(run_id) {
            Some(p) => p,
            None => {
                log::error!("[CERT:EXECUTION] Run {} failed - could not build execution plan", run_id + 1);
                break;
            }
        };

        execution_hashes.push(plan.execution_hash.clone());
        semantic_hashes.push(plan.semantic_hash.clone());

        let run_duration = run_start.elapsed().as_millis();
        log::info!("[CERT:EXECUTION] Run {}/{} completed in {}ms", run_id + 1, runs, run_duration);
        log::info!("[CERT:EXECUTION]   Execution hash: {}", plan.execution_hash.chars().take(16).collect::<String>());
        log::info!("[CERT:EXECUTION]   Semantic hash:   {}", plan.semantic_hash.chars().take(16).collect::<String>());
        log::info!("[CERT:EXECUTION]   FFmpeg: {}", plan.ffmpeg.semantic_summary());

        if run_id > 0 {
            let prev_plan = build_execution_plan(run_id - 1).unwrap_or_else(|| plan.clone());
            let plans_identical = plan.equals(&prev_plan);
            let semantic_identical = plan.semantic_equals(&prev_plan);
            let ffmpeg_args_identical = plan.ffmpeg.arguments == prev_plan.ffmpeg.arguments;
            let ffmpeg_semantic_identical = plan.ffmpeg.arguments_normalized == prev_plan.ffmpeg.arguments_normalized;
            let concat_identical = plan.concat_list.content_hash == prev_plan.concat_list.content_hash;

            let first_difference = if !plans_identical {
                Some("plan fields differ (absolute paths or environment)".to_string())
            } else if !semantic_identical {
                Some("semantic plan differs (codec, args, files)".to_string())
            } else if !ffmpeg_args_identical {
                Some("ffmpeg arguments differ (paths)".to_string())
            } else if !ffmpeg_semantic_identical {
                Some("ffmpeg semantic arguments differ".to_string())
            } else if !concat_identical {
                Some("concat content differs".to_string())
            } else {
                None
            };

            comparisons.push(ExecutionComparison {
                run_a: run_id,
                run_b: run_id + 1,
                plans_identical,
                semantic_identical,
                ffmpeg_args_identical,
                ffmpeg_semantic_identical,
                concat_identical,
                first_difference: first_difference.clone(),
            });

            let all_identical = plans_identical && semantic_identical && ffmpeg_args_identical && ffmpeg_semantic_identical && concat_identical;
            if all_identical {
                log::info!("[CERT:EXECUTION] Run {} vs Run {}: ✅ ALL IDENTICAL", run_id, run_id + 1);
            } else {
                log::error!("[CERT:EXECUTION] Run {} vs Run {}: ❌ DIFFERENCES DETECTED", run_id, run_id + 1);
                log::error!("[CERT:EXECUTION]   Plans identical: {}", plans_identical);
                log::error!("[CERT:EXECUTION]   Semantic identical: {}", semantic_identical);
                log::error!("[CERT:EXECUTION]   FFmpeg args identical: {}", ffmpeg_args_identical);
                log::error!("[CERT:EXECUTION]   FFmpeg semantic identical: {}", ffmpeg_semantic_identical);
                if let Some(ref diff) = first_difference {
                    log::error!("[CERT:EXECUTION]   First difference: {}", diff);
                }
            }
        }
    }

    let total_duration = start.elapsed().as_millis();
    let all_identical = comparisons.iter().all(|c|
        c.plans_identical && c.ffmpeg_args_identical && c.concat_identical);
    let all_semantic_identical = comparisons.iter().all(|c| c.semantic_identical);
    let passed = all_identical && comparisons.len() == runs - 1;
    let semantic_passed = all_semantic_identical && comparisons.len() == runs - 1;

    log::info!("[CERT:EXECUTION] ═══════════════════════════════════════════════════════════");
    if passed {
        log::info!("[CERT:EXECUTION] ✅ FULL CERTIFICATION PASSED - All {} runs identical", runs);
    } else {
        log::error!("[CERT:EXECUTION] ❌ FULL CERTIFICATION FAILED - Differences found");
    }
    if semantic_passed {
        log::info!("[CERT:EXECUTION] ✅ SEMANTIC CERTIFICATION PASSED - Execution intent identical");
    } else {
        log::warn!("[CERT:EXECUTION] ⚠️  SEMANTIC CERTIFICATION FAILED - Execution intent differs");
    }
    log::info!("[CERT:EXECUTION] Total duration: {}ms", total_duration);
    log::info!("[CERT:EXECUTION] ═══════════════════════════════════════════════════════════");

    ExecutionCertReport {
        total_runs: runs,
        passed,
        semantic_passed,
        execution_hashes,
        semantic_hashes,
        comparisons,
        total_duration_ms: total_duration,
    }
}

impl std::fmt::Display for ExecutionCertReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║         EXECUTION IDEMPOTENCY CERTIFICATION REPORT                  ║")?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║  Total runs: {:53} ║", self.total_runs)?;
        writeln!(f, "║  Total duration: {:47}ms ║", self.total_duration_ms)?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║  RUN COMPARISON RESULTS:")?;
        for comp in &self.comparisons {
            let status = if comp.plans_identical && comp.ffmpeg_args_identical && comp.concat_identical {
                "✅ PASS"
            } else {
                "❌ FAIL"
            };
            let semantic = if comp.semantic_identical { "✅ SEM" } else { "❌ SEM" };
            writeln!(f, "║    Run {} vs Run {}: {} {}                             ║", comp.run_a + 1, comp.run_b + 1, status, semantic)?;
            if let Some(ref diff) = comp.first_difference {
                writeln!(f, "║      {:54} ║", diff)?;
            }
        }
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        let overall = if self.passed { "✅ FULLY CERTIFIED - IDEMPOTENT" } else { "❌ FAILED - NON-IDEMPOTENT" };
        let semantic = if self.semantic_passed { "✅ SEMANTICALLY IDEMPOTENT" } else { "⚠️  SEMANTIC DIFFERENCES" };
        writeln!(f, "║  FULL: {:57} ║", overall)?;
        writeln!(f, "║  SEMANTIC: {:54} ║", semantic)?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════╝")?;
        Ok(())
    }
}

impl PartialEq for ExecutionPlan {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}