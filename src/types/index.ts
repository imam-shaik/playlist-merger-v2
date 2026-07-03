// ────────────────────────────────────────────────
// Media / Probe types
// ────────────────────────────────────────────────

export interface VideoStream {
  codecName: string;
  codecLongName: string;
  width?: number;
  height?: number;
  fps?: number;
  bitRate?: number;
  pixelFormat?: string;
  colorSpace?: string;
  colorPrimaries?: string;
  colorTransfer?: string;
  profile?: string;
  level?: number;
  duration?: number;
  timeBase?: string;
  streamIndex: number;
  /** Raw frame rate from r_frame_rate (for VFR detection) */
  rFrameRate?: string;
  /** Field order (progressive, interlaced, etc.) */
  fieldOrder?: string;
  /** Average frame rate from avg_frame_rate */
  avgFrameRate?: string;
  /** Bit depth (bits per raw sample) */
  bitsPerRawSample?: number;
  /** Sample aspect ratio */
  sampleAspectRatio?: string;
  /** Display aspect ratio */
  displayAspectRatio?: string;
  /** Rotation metadata (0, 90, 180, 270) */
  rotation?: number;
  /** Video start time in seconds (for audio offset/delay comparison) */
  startTime?: number;
}

export interface AudioStream {
  codecName: string;
  codecLongName: string;
  sampleRate?: number;
  channels?: number;
  channelLayout?: string;
  bitRate?: number;
  duration?: number;
  streamIndex: number;
  /** Audio profile (e.g., "LC" for AAC-LC, "HE" for HE-AAC) */
  profile?: string;
  /** Bits per raw sample */
  bitsPerRawSample?: number;
  /** Audio start PTS (for audio offset/delay detection) */
  startPts?: number;
  /** Audio start time in seconds (for offset/delay detection) */
  startTime?: number;
  /** Audio language */
  language?: string;
}

export interface SubtitleStream {
  codecName: string;
  codecLongName: string;
  language?: string;
  title?: string;
  streamIndex: number;
  isExternal: boolean;
  path?: string;
  /** True for bitmap-based subtitles (PGS, VobSub, DVB) that cannot be extracted to SRT */
  isBitmap?: boolean;
}

export interface MediaInfo {
  path: string;
  duration: number;
  size: number;
  formatName: string;
  formatLongName: string;
  bitRate?: number;
  videoStreams: VideoStream[];
  audioStreams: AudioStream[];
  subtitleStreams: SubtitleStream[];
  startTime?: number;
  creationTime?: string;
}

// ────────────────────────────────────────────────
// Playlist types
// ────────────────────────────────────────────────

// ────────────────────────────────────────────────
// File Health types (two-stage system)
// Stage 1: Intrinsic file health (probe-based, stored per file)
// Stage 2: Playlist overlay (comparison-based, per-job)
// ────────────────────────────────────────────────

export type FileHealthStatus =
  | 'healthy'
  | 'healthy_with_warnings'
  | 'seekability_issue'
  | 'minor_metadata_issue'
  | 'corrupted'
  | 'unreadable';

export interface FileHealth {
  /** Overall health status */
  status: FileHealthStatus;
  /** File path (from health check) */
  path?: string;
  /** Human-readable explanation of the health status */
  message: string;
  /** Technical details (decoder errors, raw ffprobe output) for tooltip/debugging */
  technicalDetails?: string;
  /** Confidence score 0-100 */
  confidence: number;
  /** Whether this file can be merged in lossless (stream-copy) mode */
  canMergeLossless: boolean;
  /** Whether this file can be merged in custom (re-encode) mode */
  canMergeCustom: boolean;
  /** Whether auto-repair (normalization) would fix the issue */
  autoRepair: boolean;
  /** Per-playlist context: file has mismatches vs dominant profile (only in playlist context) */
  outlierReason?: string;
  /** Which normalization type would be applied */
  normalizationType?: string;
  /** Whether the current merge mode would repair this file */
  repairedByCurrentMode?: boolean;
}

export interface PlaylistHealthReport {
  /** Total files in playlist */
  totalFiles: number;
  /** Count of fully healthy files */
  healthyCount: number;
  /** Count of files with minor seeking artifacts (MPEG-TS artifacts, play fine) */
  healthyWithWarningsCount: number;
  /** Count of files with significant seeking issues */
  seekabilityIssueCount: number;
  /** Count of files with metadata issues (0x0, invalid fps, etc.) */
  metadataIssueCount: number;
  /** Count of files with real structural damage */
  corruptedCount: number;
  /** Count of unreadable files */
  unreadableCount: number;
  /** Per-file health breakdown in playlist order */
  perFile: FileHealth[];
}

export interface PlaylistEntry {
  id: string;
  name: string;
  path: string;
  parentFolder?: string;
  relativePath?: string;
  extension: string;
  size: number;
  mediaInfo: MediaInfo | null;
  thumbnailPath: string | null;
  isProbing: boolean;
  isLoadingThumbnail: boolean;
  /** Stage 1 intrinsic health (null = not yet calculated) */
  health?: FileHealth | null;
  modified?: number;
  created?: number;
}

export type SortField =
  | 'name'
  | 'duration'
  | 'size'
  | 'resolution'
  | 'fps'
  | 'modified'
  | 'created'
  | 'folder'
  | 'manual';

export type SortDirection = 'asc' | 'desc';

export interface PlaylistSort {
  field: SortField;
  direction: SortDirection;
}

// ────────────────────────────────────────────────
// Repeat / Extend types
// ────────────────────────────────────────────────

/** Duration input unit for the user */
export type DurationUnit = 'minutes' | 'hours';

/** Configuration for repeating/extending the playlist output */
export interface RepeatConfig {
  /** Master enable/disable for repeat */
  enabled: boolean;
  /** Repeat by count: repeat the playlist N times */
  byCount: boolean;
  /** The repeat count value (when byCount is true) */
  repeatCount: number;
  /** Repeat until duration: extend until target total duration is reached */
  untilDuration: boolean;
  /** Target total duration in seconds (when untilDuration is true) */
  targetDurationSeconds: number;
  /** The unit the user is currently typing in (UI only, not sent to backend) */
  durationUnit: DurationUnit;
  /** Insert a boundary card at the start of each repeat cycle */
  insertBoundaryCards: boolean;
  /** Template for the boundary card label. {n} is replaced with cycle number. */
  boundaryCardTemplate: string;
}

/** Default repeat config (disabled) */
export const DEFAULT_REPEAT_CONFIG: RepeatConfig = {
  enabled: false,
  byCount: false,
  repeatCount: 5,
  untilDuration: false,
  targetDurationSeconds: 1800,
  durationUnit: 'minutes',
  insertBoundaryCards: false,
  boundaryCardTemplate: '🔁 Repeat {n}',
};

// ────────────────────────────────────────────────
// Merge types
// ────────────────────────────────────────────────

/** Lossless = stream copy only. Custom = user-configured re-encode. */
export type MergeMode = 'lossless' | 'custom' | 'fastMkv' | 'smartMkv';

/** Audio repair strategy used by lossless merges */
export type AudioRepairMode = 'fast' | 'smart' | 'safe';

/**
 * Strategy for handling audio validation when a large playlist is detected.
 * Only applies when audioRepairMode = 'smart' AND the playlist exceeds
 * 300 files OR 24 hours total duration.
 */
export type LargePlaylistStrategy = 'fullSmart' | 'smartLite' | 'safe' | 'fast';

/** How subtitles are handled in the merge output */
export type SubtitleMode = 'none' | 'embed' | 'burn' | 'exportSrt' | 'srtMergeOnly';

/** Default: embed subtitles as a track in the output video */
export const DEFAULT_SUBTITLE_MODE: SubtitleMode = 'embed';

export interface MergeSegment {
  name: string;
  duration: number;
  startTime: number;
  endTime: number;
  /** Whether this segment is a canvas overlay card */
  isCard?: boolean;
  /** Card background color (only set when isCard is true) */
  cardColor?: string;
  parentFolder?: string;
}

export type MergePhase =
  | 'preparing'
  | 'probing'
  | 'validating'
  | 'normalizing'
  | 'writing'
  | 'finalizing'
  | 'complete'
  | 'failed'
  | 'cancelled';

/** Per-phase timing for a completed merge (seconds). */
export interface PhaseTimes {
  probing: number;
  validating: number;
  preparing: number;
  normalizing: number;
  writing: number;
  finalizing: number;
}

/** A completed merge record for historical ETA learning. Stored in AppSettings. */
export interface MergeStatsRecord {
  id: string;
  files: number;
  totalMediaDurationSeconds: number;
  mode: MergeMode;
  audioRepairMode: AudioRepairMode;
  largePlaylistStrategy?: LargePlaylistStrategy;
  subtitleMode: SubtitleMode;
  totalTimeSeconds: number;
  phaseTimes: Partial<PhaseTimes>;
  completedAt: number;
}

/** Weighted estimates per phase — used for phase-based ETA calculation. */
export interface PhaseEstimate {
  phase: MergePhase;
  elapsedSeconds: number;
  estimatedRemainingSeconds: number;
  weight: number;
}

export interface MergeProgress {
  percent?: number;
  currentTime: number;
  totalDuration: number;
  speed?: number;
  fps?: number;
  currentFile?: string;
  currentSegmentIndex?: number;
  remainingDuration?: number;
  bytesWritten?: number;
  etaSeconds?: number;
  phase: MergePhase;
  overallPercent?: number;
  stageName?: string;
  stagePercent?: number;
  currentFileIndex?: number;
  totalFilesInStage?: number;
  /** Normalization action being performed: "Timebase Fix", "Audio Bitrate Repair", "Lossless Remux", etc. */
  normalizationType?: string;
  /** Reason for repair/re-normalization (e.g. "ProfileMismatch_Video", "Corruption_Detected") */
  repairReason?: string;
  /** Warning message to display in the UI (e.g. "Large playlist detected") */
  warning?: string;
  /** Whether this is a large playlist (auto-detected: >300 files or >24h duration) */
  isLargePlaylist?: boolean;
  /** Large playlist strategy used (Smart mode only) */
  largePlaylistStrategy?: string;
  /** Phase 1: Normalization plan emitted at start of normalizing phase */
  normalizationPlan?: NormalizationPlan;
  /** Phase 5: Smart MKV analysis breakdown (only in SmartMkv mode) */
  smartMkvBreakdown?: SmartMkvBreakdown;
}

/** Per-file normalization classification for the Normalization Dashboard */
export interface NormalizationClassification {
  index: number;
  filename: string;
  /** Human-readable type: "Audio", "Video", or "Audio + Video" */
  type: string;
  /** Badge color: "yellow", "blue", "purple", or "green" */
  badge: 'yellow' | 'blue' | 'purple' | 'green';
  inProfile: boolean;
  inAudio: boolean;
}

/** Smart MKV analysis breakdown from the backend */
export interface SmartMkvBreakdown {
  willNormalize: number;
  willRemux: number;
  willSkip: number;
  totalFiles: number;
  categories: {
    normalize: { property: string; count: number }[];
    remux: { property: string; count: number }[];
    skip: { property: string; count: number }[];
  };
}

/** Summary of normalization plan emitted before normalization starts */
export interface NormalizationPlan {
  /** Total files in the merge */
  totalFiles: number;
  /** Files that don't need normalization */
  normalCount: number;
  /** Files needing audio-only repair */
  audioOnlyCount: number;
  /** Files needing video-only normalization */
  videoOnlyCount: number;
  /** Files needing both audio and video normalization */
  audioVideoCount: number;
  /** Per-file classification list */
  classifications: NormalizationClassification[];
}

/** Structured per-file progress events emitted during audio validation and seek checks. */
export interface MergeFileProgress {
  jobId: string;
  phase: 'validating' | 'normalizing';
  fileIndex: number;
  totalFiles: number;
  filename: string;
  duration: number;
  seekPoints: number;
  maxGap: number;
  result?: 'pass' | 'fail' | 'running';
  /** Reason if result is 'fail' (e.g. "SeekTimeError", "DecoderError") */
  failReason?: string;
}

export interface MergeRequest {
  jobId: string;
  /** Resume phase — allows backend to skip already-completed phases */
  phase?: MergePhase;
  inputFiles: string[];
  mediaInfos?: MediaInfo[];
  inputNames: string[]; // Added for report generation
  inputDurations: number[]; // Matching index with inputFiles
  inputThumbnails?: (string | null)[]; // Matching index with inputFiles — thumbnail paths for report display
  externalSubtitles?: (string | null)[]; // Matching index with inputFiles
  outputPath: string;
  mode: MergeMode;
  totalDuration: number;
  /** Subtitle handling mode: none | embed | burn | exportSrt */
  subtitleMode?: SubtitleMode;
  /** When true, also generate a standalone merged SRT file alongside the video */
  exportMergedSrt?: boolean;
  /** Per-file selected embedded subtitle stream indices (null/undefined = auto-select first) */
  selectedSubtitleStreamIndices?: (number | null)[];
  // Custom mode options
  videoCodec?: string;
  audioCodec?: string;
  videoCrf?: number;
  videoPreset?: string;
  audioBitrate?: string;
  targetResolution?: string;
  targetFps?: string;
  hwAccel?: string;
  // Canvas overlay cards between merged videos
  cardConfig?: CardConfig;
  // Repeat / Extend options
  repeatConfig?: RepeatConfig;
  // Split options
  splitConfig?: SplitConfig;
  /** Output naming configuration for split parts */
  namingConfig?: NamingConfig;
  /** Run deep audio validation (decoder test) before merging */
  validateAudio?: boolean;
  /** Audio repair strategy for lossless merge */
  audioRepairMode?: AudioRepairMode;
  /**
   * Strategy for large playlist audio validation.
   * When audioRepairMode='smart' and a large playlist is detected,
   * this determines which validation path to take.
   * If not set, the user's default (from settings) is used, or SmartLite as fallback.
   */
  largePlaylistStrategy?: LargePlaylistStrategy;
}

/** Configuration for splitting merge output into multiple files */
export interface SplitConfig {
  /** Split mode: 'none' | 'count' | 'duration' | 'folder' */
  mode: 'none' | 'count' | 'duration' | 'folder';
  /** Sub-mode for folder mode: how to handle each folder's output */
  folderSplitMode?: 'single' | 'parts';
  /** Number of parts (when mode = 'count' or mode = 'folder' with folderSplitMode = 'parts') */
  partCount?: number;
  /** Maximum duration per part in seconds (when mode = 'duration') */
  maxDurationPerPart?: number;
  /** Subtitle handling for split output: 'embed' | 'exportSrt' | 'ignore' */
  subtitleMode?: 'embed' | 'exportSrt' | 'ignore';
}

/** Result of a merge operation - can contain multiple output files if split */
export interface MergeResult {
  jobId: string;
  outputPath: string;
  outputSizeBytes: number;
  /** ffprobe-reported duration of the output file (may be corrupted for MKV with mixed sample rates) */
  outputDurationSecs?: number;
  segments: MergeSegment[];
  /** When split, contains all output file paths */
  outputPaths?: string[];
  /** Individual part results when split */
  parts?: MergePartResult[];
  /** Paths to the generated _report.txt files (one per output file) */
  reportPaths?: string[];
  /** Paths to generated merged SRT files (one per output part) */
  srtExportPaths?: string[];
  /** Warnings accumulated during the merge process (e.g. burn fallback) */
  warnings?: string[];
  /** Summary of audio files that were detected as problematic and re-encoded during the merge */
  audioRepairSummary?: AudioRepairSummary;
  /** The merge mode originally requested by the user */
  requestedMode?: string;
  /** The merge mode actually used by the backend (may differ if auto-upgraded) */
  actualMode?: string;
  /** Human-readable reason for auto-upgrade, if applicable */
  upgradeReason?: string;
}

/** Summary of audio repair activity during a merge */
export interface AudioRepairSummary {
  mode: string;
  totalFiles: number;
  filesRepaired: number;
  dueToCorruption: number;
  dueToProfileMismatch: number;
  dueToSafeMode: number;
  repairedIndices: number[];
}

/** Result of a single part in a split merge */
export interface MergePartResult {
  partIndex: number;
  outputPath: string;
  outputSizeBytes: number;
  fileCount: number;
  totalDuration: number;
}

export interface MergeLogEntry {
  timestamp: number;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
  /** Optional file name this log relates to (for linking to specific files) */
  fileName?: string;
  /** Optional technical details (ffprobe output, codec info, etc.) */
  details?: string;
}

export interface MergeJob {
  id: string;
  request: MergeRequest;
  progress: MergeProgress;
  result?: MergeResult;
  error?: string;
  startedAt: number;
  completedAt?: number;
  /** Live activity log entries for this job */
  logs: MergeLogEntry[];
  /** Per-phase elapsed times tracked during this merge. */
  phaseTimes?: Partial<PhaseTimes>;
  /** Subtitle extraction warnings collected during merge */
  subtitleWarnings: SubtitleWarning[];
}

/** Subtitle extraction warning — emitted when subtitle extraction fails for a file */
export interface SubtitleWarning {
  fileIndex: number;
  filePath: string;
  reason: string;
}

/** Frequency of canvas overlay cards */
export type CardFrequency = 'perVideo' | 'perFolder';

/** Configuration for canvas overlay cards between merged videos */
export interface CardConfig {
  /** Background color in hex (e.g. "#3366FF") */
  color: string;
  /** Font color in hex (auto-computed from luminance) */
  fontColor: string;
  /** Duration of each card in seconds */
  duration: number;
  /** Whether to show canvas entries in the merge report */
  showInReport: boolean;
  /** Frequency of cards: between every video or only on folder change */
  frequency: CardFrequency;
}

// ────────────────────────────────────────────────
// Compatibility types
// ────────────────────────────────────────────────

export type IssueKind =
  | 'videoCodecMismatch'
  | 'audioCodecMismatch'
  | 'subtitleCodecMismatch'
  | 'resolutionMismatch'
  | 'fpsMismatch'
  | 'pixelFormatMismatch'
  | 'sampleRateMismatch'
  | 'channelMismatch'
  | 'containerMismatch'
  | 'timebaseMismatch';

export type IssueSeverity = 'error' | 'warning' | 'info';

export interface CompatibilityIssue {
  kind: IssueKind;
  severity: IssueSeverity;
  description: string;
  affectedFiles: string[];
}

export interface CompatibilityReport {
  isCompatible: boolean;
  issues: CompatibilityIssue[];
  recommendedMode: MergeMode;
}

// ────────────────────────────────────────────────
// Settings types
// ────────────────────────────────────────────────

export interface RecentExport {
  path: string;
  timestamp: string;
  sizeBytes: number;
  fileCount: number;
  durationSeconds: number;
  mode: string;
}

export interface AppSettings {
  ffmpegPath?: string;
  ffprobePath?: string;
  lastExportDir?: string;
  thumbnailCacheDir?: string;
  maxThumbnailCacheMb: number;
  recentExports: RecentExport[];
  defaultMergeMode: MergeMode;
  checkCompatBeforeMerge: boolean;
  autoSavePlaylist: boolean;
  /** Default strategy for large playlist audio validation (Smart mode only). */
  largePlaylistDefault?: LargePlaylistStrategy;
  /** Historical merge stats for phase-weighted ETA learning. Kept to last 20. */
  mergeStatsHistory?: MergeStatsRecord[];
}

// ────────────────────────────────────────────────
// File system types
// ────────────────────────────────────────────────

export interface ScannedFile {
  path: string;
  name: string;
  size: number;
  extension: string;
  modified?: number;
  created?: number;
  parentFolder?: string;
  relativePath?: string;
}

export interface DiskSpaceInfo {
  availableBytes: number;
  totalBytes: number;
  path: string;
}

export interface FfmpegPaths {
  ffmpeg?: string;
  ffprobe?: string;
  ffmpegFound: boolean;
  ffprobeFound: boolean;
  ffmpegError?: string;
  ffprobeError?: string;
}

export type AppScreen = 'playlist' | 'merge' | 'repeat' | 'settings' | 'split';

// ────────────────────────────────────────────────
// Screenshot types
// ────────────────────────────────────────────────

export interface Screenshot {
  id: string;
  /** Path to the captured frame image on disk */
  imagePath: string;
  /** Source video file path */
  sourcePath: string;
  /** Source video filename (display name) */
  sourceName: string;
  /** Timestamp in seconds when the frame was captured */
  timestamp: number;
  /** ISO string of when the screenshot was taken */
  capturedAt: string;
  /** User notes attached to this screenshot */
  notes: string;
}

// ────────────────────────────────────────────────
// Folder types
// ────────────────────────────────────────────────

export interface Folder {
  id: string;
  path: string;
  name: string;
  order: number;
  isCollapsed: boolean;
}

export interface FolderGroup {
  folder: Folder;
  entryIds: string[];
  duration: number;
}

// ────────────────────────────────────────────────
// Split types
// ────────────────────────────────────────────────

export type SplitMode =
  | 'byParts'
  | 'byDuration'
  | 'byChapters'
  | 'customRanges'
  | 'byPlaylistItems'
  | 'byOutputSize'
  | 'smartCourse';

export interface SplitSegment {
  index: number;
  label: string;
  startTime: number;
  endTime: number;
  duration: number;
  estimatedSizeBytes?: number;
}

export interface SplitPlan {
  jobId: string;
  inputFile: string;
  inputDuration: number;
  inputSizeBytes: number;
  mode: SplitMode;
  segments: SplitSegment[];
  outputDir: string;
  outputFormat: string;
}

export interface SplitParams {
  partCount?: number;
  partDuration?: number;
  customRanges?: number[];
  itemsPerSegment?: number;
  maxSizeBytes?: number;
  courseMode?: 'daily' | 'weekly';
  hoursPerUnit?: number;
  outputFormat?: string;
  labelPrefix?: string;
  /** Subtitle handling: 'copyAll' | 'extractSplit' | 'ignore' */
  subtitleMode?: 'copyAll' | 'extractSplit' | 'ignore';
  /** Export SRT files per segment (only applies when subtitleMode='extractSplit') */
  exportSrt?: boolean;
  /** Output naming configuration */
  namingConfig?: NamingConfig;
}

export interface SplitPlanRequest {
  jobId: string;
  inputFile: string;
  inputDuration: number;
  inputSizeBytes: number;
  mode: SplitMode;
  params: SplitParams;
  outputDir: string;
}

export interface SplitExecuteRequest {
  jobId: string;
  plan: SplitPlan;
  subtitleMode?: 'copyAll' | 'extractSplit' | 'ignore';
  exportSrt?: boolean;
  namingConfig?: NamingConfig;
}

export interface SplitResult {
  jobId: string;
  outputPaths: string[];
  outputSizesBytes: number[];
  totalDuration: number;
  segmentsCount: number;
  /** Paths to generated SRT files (one per segment) */
  srtOutputPaths?: string[];
  /** Paths to generated report files, if any. */
  reportPaths?: string[];
}

export interface SplitProgress {
  jobId: string;
  segmentIndex: number;
  segmentCount: number;
  progress: number;
  stage: string;
  message: string;
}

export type SplitStage = 'idle' | 'planning' | 'preview' | 'splitting' | 'complete' | 'failed' | 'cancelled';

export interface SplitJob {
  id: string;
  plan: SplitPlan;
  progress: SplitProgress;
  result?: SplitResult;
  error?: string;
  stage: SplitStage;
  startedAt: number;
  completedAt?: number;
}

// ────────────────────────────────────────────────
// Naming system types
// ────────────────────────────────────────────────

export type NamingModeId =
  | 'sequential' | 'prefix' | 'suffix' | 'custom'
  | 'timestamp' | 'chapter' | 'playlist' | 'date' | 'smart_course';

export type TemplateVariable =
  | 'filename' | 'ext' | 'num' | 'num2' | 'num3' | 'num4'
  | 'date' | 'time' | 'start' | 'end' | 'duration'
  | 'chapter' | 'part_label' | 'resolution' | 'height' | 'width'
  | 'folder' | 'playlist' | 'playlist_index' | 'original_num'
  | 'video_count' | 'total_duration'
  | 'lang' | 'lang_name'
  | 'prefix' | 'suffix';

export interface NamingMode {
  id: NamingModeId;
  label: string;
  description: string;
  defaultTemplate: string;
  example: { input: string; output: string };
  supportedVariables: TemplateVariable[];
}

export interface NamingConfig {
  mode: NamingModeId;
  template: string;
  prefix?: string;
  suffix?: string;
  zeroPadding?: 2 | 3 | 4;
  separator?: string;
}

export interface NamingError {
  code: string;
  message: string;
  position?: number;
}

export interface NamingWarning {
  code: string;
  message: string;
}

export interface NamingValidation {
  valid: boolean;
  errors: NamingError[];
  warnings: NamingWarning[];
  resolvedPreview?: string;
  batchPreview?: string[];
  totalCount?: number;
}

export interface TemplateContext {
  filename: string;
  extension: string;
  folder?: string;
  originalNum?: string;
  index: number;
  startTime: number;
  endTime: number;
  duration: number;
  chapter?: string;
  resolution?: string;
  width?: number;
  height?: number;
  playlist?: string;
  playlistIndex?: number;
  videoCount?: number;
  totalDuration?: number;
  date: string;
  time: string;
  prefix?: string;
  suffix?: string;
  lang?: string;
  langName?: string;
  partLabel?: string;
}

// ────────────────────────────────────────────────
// Recovery types
// ────────────────────────────────────────────────

export type NormalizationType = 'Full' | 'Audio' | 'Timescale';

export interface CompletedFile {
  index: number;
  sourcePath: string;
  sourceSize: number;
  sourceMtime: number;
  normalizedPath: string;
  normalizationType: NormalizationType;
}

export interface DominantProfile {
  vCodec?: string;
  vWidth?: number;
  vHeight?: number;
  vFps?: number;
  aCodec?: string;
  aSampleRate?: number;
  aChannels?: number;
  timescaleDen?: number;
}

export interface RecoveryCheckpoint {
  version: number;
  jobId: string;
  phase: string;
  startedAt: number;
  inputFiles: string[];
  outputPath: string;
  mode: string;
  dominantProfile: DominantProfile;
  completedFiles: CompletedFile[];
  remainingIndices: number[];
  // v1 fields
  repeatConfig?: RepeatConfig;
  originalFileCount?: number;
  repeatCount?: number;
  // v2 fields
  subtitleMode?: SubtitleMode;
  exportMergedSrt?: boolean;
  selectedSubtitleStreamIndices?: (number | null)[];
  videoCodec?: string;
  audioCodec?: string;
  videoCrf?: number;
  videoPreset?: string;
  audioBitrate?: string;
  targetResolution?: string;
  targetFps?: string;
  hwAccel?: string;
  cardConfig?: CardConfig;
  splitConfig?: SplitConfig;
  namingConfig?: NamingConfig;
  audioRepairMode?: AudioRepairMode;
  validateAudio?: boolean;
  largePlaylistStrategy?: LargePlaylistStrategy;
  convertToMp4?: boolean;
  // P0: Duration persistence for resume without re-probing
  inputDurations?: number[];
  totalDuration?: number;
}

export interface RecoveryCheckResult {
  checkpoints: RecoveryCheckpoint[];
}

export interface SectionBoundary {
  sectionIndex: number;
  folderStartIdx: number;
  folderEndIdx: number;
  videoCount: number;
  durationSecs: number;
  estimatedSizeBytes: number;
  folderNames: string[];
}

export interface SectionPlan {
  sectionIndex: number;
  boundary: SectionBoundary;
  outputName: string;
}

export type PartitionMethod =
  | { sectionCount: number }
  | { maxDurationSecs: number }
  | { maxSizeBytes: number };

export interface FolderForMerge {
  name: string;
  path: string;
  videoCount: number;
  durationSecs: number;
  sizeBytes: number;
  filePaths: string[];
  includeInSections: boolean;
}

export interface SectionMergeConfig {
  method: PartitionMethod;
  nameTemplate: string;
  outputSubfolder: string;
  outputBaseDir: string;
  baseMergeMode: string;
  normalizeAudio: boolean;
  quality?: string;
  largePlaylistStrategy?: string;
}

export interface SectionMergeRequest {
  jobId: string;
  config: SectionMergeConfig;
  folders: FolderForMerge[];
}

export interface SectionEvent {
  jobId: string;
  currentSection: number;
  totalSections: number;
  sectionName: string;
  phase: string;
  progressPercent?: number;
  etaSeconds?: number;
  error?: string;
}

export interface SectionResult {
  sectionIndex: number;
  outputPath: string;
  durationSecs: number;
  sizeBytes: number;
  success: boolean;
  errorMessage?: string;
}

export interface SectionMergeStatus {
  isRunning: boolean;
  runningJobIds: string[];
}
