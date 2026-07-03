import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
import type {
  ScannedFile, MediaInfo, DiskSpaceInfo, AppSettings,
  MergeRequest, MergeProgress, FfmpegPaths, MergeSegment,
  SplitPlanRequest, SplitPlan, SplitExecuteRequest, SplitResult, SplitProgress,
  NamingConfig, NamingValidation, TemplateContext,
  PlaylistHealthReport, AudioRepairSummary,
  RecoveryCheckpoint,
  SectionPlan, PartitionMethod, FolderForMerge,
  SectionMergeRequest, SectionEvent, SectionResult,
} from '@/types';

// ─── File system ──────────────────────────────────────────────────────────────

export interface ScanOptions {
  recursive: boolean;
  maxDepth?: number;
  sortBy?: 'name' | 'modified' | 'created' | 'size';
}

export interface SubfolderInfo {
  name: string;
  path: string;
  videoCount: number;
  hasSubfolders: boolean;
}

export const tauriCommands = {
  scanDirectory: (path: string, options: ScanOptions): Promise<ScannedFile[]> =>
    invoke('scan_directory', { path, options }),

  scanSubfolders: (path: string): Promise<SubfolderInfo[]> =>
    invoke('scan_subfolders', { path }),

  getDiskSpace: (path: string): Promise<DiskSpaceInfo> =>
    invoke('get_disk_space', { path }),

  revealInExplorer: (path: string): Promise<void> =>
    invoke('reveal_in_explorer', { path }),

  openWithDefault: (path: string): Promise<void> =>
    invoke('open_with_default', { path }),

  sanitizeFilename: (name: string): Promise<string> =>
    invoke('sanitize_filename', { name }),

  ensureDirectory: (path: string): Promise<void> =>
    invoke('ensure_directory', { path }),

  // ─── Media ──────────────────────────────────────────────────────────────────

  probeVideo: (path: string): Promise<MediaInfo> =>
    invoke('probe_video', { path }),

  /** Returns array of MediaInfo | string (error message) per path */
  batchProbe: async (paths: string[]): Promise<Array<MediaInfo | string>> => {
    const raw = await invoke<Array<{ Ok: MediaInfo } | { Err: string }>>('batch_probe', { paths });
    return raw.map((res) => ('Ok' in res ? res.Ok : res.Err));
  },

  generateThumbnail: (
    videoPath: string,
    timestampSeconds?: number,
    size?: { width: number; height: number },
  ): Promise<string> =>
    invoke('generate_thumbnail', { videoPath, timestampSeconds, size }),

  clearThumbnailCache: (): Promise<number> =>
    invoke('clear_thumbnail_cache'),

  /** Capture a full-resolution frame from a video at a given timestamp */
  captureFrame: (
    videoPath: string,
    timestampSeconds?: number,
  ): Promise<string> =>
    invoke('capture_frame', { videoPath, timestampSeconds }),

  // ─── Merge ──────────────────────────────────────────────────────────────────

  startMerge: (request: MergeRequest): Promise<string> =>
    invoke('start_merge', { request }),

  cancelMerge: (jobId: string): Promise<void> =>
    invoke('cancel_merge', { jobId }),

  getMergeStatus: (): Promise<{ isRunning: boolean; currentJobId?: string }> =>
    invoke('get_merge_status'),

  validateAudioFiles: (files: string[]): Promise<{
    totalFiles: number;
    passedFiles: number;
    failedFiles: number;
    errors: Array<{
      index: number;
      filename: string;
      filepath: string;
      errors: string[];
    }>;
  }> =>
    invoke('validate_audio_files', { files }),

  checkMergeCompatibility: (
    inputFiles: string[],
    mediaInfos: import('@/types').MediaInfo[],
    selectedMode: string,
  ): Promise<{
    canMergeLossless: boolean;
    autoUpgradeReason: string | null;
    incompatibleFiles: Array<{
      fileIndex: number;
      filename: string;
      reason: string;
      severity: string;
    }>;
    losslessWillApply: boolean;
    warnings: Array<{
      fileIndex: number;
      filename: string;
      reason: string;
      severity: string;
    }>;
  }> => invoke('check_merge_compatibility', {
    inputFiles,
    mediaInfos,
    selectedMode,
  }),

  // ─── Playlist ───────────────────────────────────────────────────────────────

  savePlaylist: (id: string, name: string, data: unknown): Promise<void> =>
    invoke('save_playlist', { id, name, data }),

  loadPlaylist: (id: string): Promise<unknown> =>
    invoke('load_playlist', { id }),

  listSavedPlaylists: (): Promise<Array<{
    id: string;
    name: string;
    createdAt: string;
    updatedAt: string;
    fileCount: number;
    totalDuration: number;
  }>> =>
    invoke('list_saved_playlists'),

  deletePlaylist: (id: string): Promise<void> =>
    invoke('delete_playlist', { id }),

  // ─── Filesystem watcher ─────────────────────────────────────────────────────

  watchDirectories: (dirs: string[]): Promise<void> =>
    invoke('watch_directories', { dirs }),

  stopWatchingDirs: (): Promise<void> =>
    invoke('stop_watching_dirs'),

  /// Cancel all pending long-running operations (probes, thumbnails, scans).
  /// Call on screen change or component unmount to prevent "callback id" errors.
  cancelPendingOperations: (): Promise<void> =>
    invoke('cancel_pending_operations'),

  // ─── Split ──────────────────────────────────────────────────────────────────

  generateSplitPlan: (request: SplitPlanRequest): Promise<SplitPlan> =>
    invoke('generate_split_plan', { request }),

  generateChapterSplitPlan: (request: SplitPlanRequest): Promise<SplitPlan> =>
    invoke('generate_chapter_split_plan', { request }),

  executeSplitPlan: (request: SplitExecuteRequest): Promise<SplitResult> =>
    invoke('execute_split_plan', { request }),

  cancelSplit: (jobId: string): Promise<void> =>
    invoke('cancel_split', { jobId }),

  // ─── Naming ─────────────────────────────────────────────────────────────────

  validateNamingTemplate: (
    template: string,
    config: NamingConfig,
    contexts: TemplateContext[],
    totalCount?: number,
  ): Promise<NamingValidation> =>
    invoke('validate_naming_template', { template, config, contexts, totalCount }),

  resolveNaming: (
    template: string,
    config: NamingConfig,
    context: TemplateContext,
  ): Promise<string> =>
    invoke('resolve_naming', { template, config, context }),

  previewNamingBatch: (
    template: string,
    config: NamingConfig,
    contexts: TemplateContext[],
  ): Promise<string[]> =>
    invoke('preview_naming_batch', { template, config, contexts }),

  writeTextFile: (path: string, content: string): Promise<void> =>
    invoke('write_text_file', { path, content }),

  // ─── Settings ───────────────────────────────────────────────────────────────

  getSettings: (): Promise<AppSettings> =>
    invoke('get_settings'),

  saveSettings: (settings: AppSettings): Promise<void> =>
    invoke('save_settings', { settings }),

  getFfmpegPath: (): Promise<FfmpegPaths> =>
    invoke('get_ffmpeg_path'),

  // ─── Health ──────────────────────────────────────────────────────────────────

  checkFileHealth: (paths: string[]): Promise<PlaylistHealthReport> =>
    invoke('check_file_health', { filePaths: paths }),

  // ─── Recovery ──────────────────────────────────────────────────────────────────

  checkRecoveryCheckpoints: (): Promise<RecoveryCheckpoint[]> =>
    invoke('check_recovery_checkpoints'),

  deleteRecoveryCheckpoint: (jobId: string): Promise<void> =>
    invoke('delete_recovery_checkpoint', { jobId }),

  // ─── Job Logs ─────────────────────────────────────────────────────────────────

  /** Open the logs folder for a job's output directory in the file explorer */
  openLogsFolder: (outputPath: string): Promise<void> =>
    invoke('open_logs_folder', { outputPath }),

  /** Get the path to the currently active job's log file */
  getActiveLogPath: (): Promise<string | null> =>
    invoke('get_active_log_path'),

  /** Get all log files in a job's output directory */
  getJobLogFiles: (outputPath: string): Promise<Array<{
    filename: string;
    path: string;
    sizeBytes: number;
    modifiedTimestamp: number;
  }>> => invoke('get_job_log_files', { outputPath }),

  /** Read the contents of a specific log file */
  readJobLog: (logPath: string): Promise<string> =>
    invoke('read_job_log', { logPath }),
};

// ─── Event listeners ─────────────────────────────────────────────────────────

export interface MergeProgressEvent {
  jobId: string;
  progress: MergeProgress;
}

export interface MergeCompleteEvent {
  jobId: string;
  outputPath: string;
  outputSizeBytes: number;
  /** ffprobe-reported duration of the output file (may be corrupted for MKV with mixed sample rates) */
  outputDurationSecs?: number;
  segments: MergeSegment[];
  outputPaths?: string[];
  parts?: Array<{
    partIndex: number;
    outputPath: string;
    outputSizeBytes: number;
    fileCount: number;
    totalDuration: number;
  }>;
  reportPaths?: string[];
  /** Paths to generated merged SRT files */
  srtExportPaths?: string[];
  /** Warnings from the merge process (e.g. burn fallback, SRT export failures) */
  warnings?: string[];
  /** Summary of audio files that were detected as problematic and re-encoded during the merge */
  audioRepairSummary?: AudioRepairSummary;
  /** The merge mode originally requested by the user (e.g. "Lossless") */
  requestedMode?: string;
  /** The merge mode actually used by the backend (may differ from requestedMode if auto-upgraded) */
  actualMode?: string;
  /** Human-readable reason why the mode was auto-upgraded, if applicable */
  upgradeReason?: string;
}

export interface MergeErrorEvent {
  jobId: string;
  error: string;
  cancelled: boolean;
  phase?: string;
}

/** Per-file progress during audio validation and seek checks. */
export interface MergeFileProgressEvent {
  jobId: string;
  phase: 'validating' | 'normalizing';
  fileIndex: number;
  totalFiles: number;
  filename: string;
  duration: number;
  seekPoints: number;
  maxGap: number;
  result?: 'pass' | 'fail' | 'running';
  failReason?: string;
}

/** Subtitle extraction warning — emitted when subtitle extraction fails for a file. */
export interface SubtitleWarningEvent {
  jobId: string;
  fileIndex: number;
  file: string;
  error: string;
}

// ─── Filesystem watcher ──────────────────────────────────────────────────────

export interface FsChangeEvent {
  paths: string[];
  kind: 'created' | 'modified' | 'renamed' | 'deleted';
}

export interface SplitCompleteEvent {
  jobId: string;
  outputPaths: string[];
  totalSegments: number;
}

export interface SplitErrorEvent {
  jobId: string;
  error: string;
}

export const tauriEvents = {
  onMergeProgress: (handler: (e: MergeProgressEvent) => void): Promise<UnlistenFn> =>
    listen<MergeProgressEvent>('merge-progress', (e) => handler(e.payload)),

  onMergeComplete: (handler: (e: MergeCompleteEvent) => void): Promise<UnlistenFn> =>
    listen<MergeCompleteEvent>('merge-complete', (e) => handler(e.payload)),

  onMergeError: (handler: (e: MergeErrorEvent) => void): Promise<UnlistenFn> =>
    listen<MergeErrorEvent>('merge-error', (e) => handler(e.payload)),

  onMergeFileProgress: (handler: (e: MergeFileProgressEvent) => void): Promise<UnlistenFn> =>
    listen<MergeFileProgressEvent>('merge-file-progress', (e) => handler(e.payload)),

  onSubtitleWarning: (handler: (e: SubtitleWarningEvent) => void): Promise<UnlistenFn> =>
    listen<SubtitleWarningEvent>('subtitle-warning', (e) => handler(e.payload)),

  /// Fired by the filesystem watcher when video/subtitle files change
  onFsChange: (handler: (e: FsChangeEvent) => void): Promise<UnlistenFn> =>
    listen<FsChangeEvent>('fs-change', (e) => handler(e.payload)),

  onSplitProgress: (handler: (e: SplitProgress) => void): Promise<UnlistenFn> =>
    listen<SplitProgress>('split-progress', (e) => handler(e.payload)),

  onSplitComplete: (handler: (e: SplitCompleteEvent) => void): Promise<UnlistenFn> =>
    listen<SplitCompleteEvent>('split-complete', (e) => handler(e.payload)),

  onSplitError: (handler: (e: SplitErrorEvent) => void): Promise<UnlistenFn> =>
    listen<SplitErrorEvent>('split-error', (e) => handler(e.payload)),
};

// ─── Section Merge ─────────────────────────────────────────────────────────────

export const sectionCommands = {
  computeSectionPreview: (
    folders: FolderForMerge[],
    method: PartitionMethod,
    nameTemplate: string,
  ): Promise<SectionPlan[]> =>
    invoke('compute_section_preview_cmd', { folders, method, nameTemplate }),

  getPlaylistHash: (folders: FolderForMerge[]): Promise<string> =>
    invoke('get_playlist_hash', { folders }),

  startSectionMerge: (request: SectionMergeRequest): Promise<string> =>
    invoke('start_section_merge', { request }),

  cancelSectionMerge: (jobId: string): Promise<void> =>
    invoke('cancel_section_merge', { jobId }),

  getSectionMergeStatus: (): Promise<{ isRunning: boolean; runningJobIds: string[] }> =>
    invoke('get_section_merge_status'),

  sectionMergeDiskSpaceRequired: (
    sectionSizeBytes: number,
    largestSectionBytes: number,
  ): Promise<number> =>
    invoke('section_merge_disk_space_required', { sectionSizeBytes, largestSectionBytes }),

  // Event listeners
  onSectionStart: (handler: (e: SectionEvent) => void): Promise<UnlistenFn> =>
    listen<SectionEvent>('section-start', (e) => handler(e.payload)),

  onSectionProgress: (handler: (e: SectionEvent) => void): Promise<UnlistenFn> =>
    listen<SectionEvent>('section-progress', (e) => handler(e.payload)),

  onSectionComplete: (handler: (e: SectionEvent) => void): Promise<UnlistenFn> =>
    listen<SectionEvent>('section-complete', (e) => handler(e.payload)),

  onSectionError: (handler: (e: SectionEvent) => void): Promise<UnlistenFn> =>
    listen<SectionEvent>('section-error', (e) => handler(e.payload)),

  onSectionMergeComplete: (handler: (e: { jobId: string; success: boolean; successCount?: number; totalCount?: number; results?: SectionResult[] }) => Promise<void>): Promise<UnlistenFn> =>
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    listen('section-merge-complete', (e) => handler(e.payload as any)),

  onSectionMergeError: (handler: (e: { jobId: string; error: string }) => Promise<void>): Promise<UnlistenFn> =>
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    listen('section-merge-error', (e) => handler(e.payload as any)),
};

// ─── Dialog helpers ──────────────────────────────────────────────────────────

export async function openVideoFilesDialog(): Promise<string[]> {
  const result = await openDialog({
    multiple: true,
    filters: [{
      name: 'Video Files',
      extensions: ['mp4', 'mkv', 'mov', 'avi', 'webm', 'm4v', 'ts', 'mts', 'flv', 'wmv', '3gp'],
    }],
  });
  if (!result) return [];
  return Array.isArray(result) ? result as string[] : [result as string];
}

export async function openFolderDialog(): Promise<string | null> {
  const result = await openDialog({ directory: true, multiple: false });
  if (!result) return null;
  return Array.isArray(result) ? result[0] as string : result as string;
}

export async function openExecutableDialog(title: string): Promise<string | null> {
  const isWin = navigator.userAgent.toLowerCase().includes('win');
  const result = await openDialog({
    multiple: false,
    directory: false,
    title,
    filters: isWin ? [{
      name: 'Executables (*.exe)',
      extensions: ['exe'],
    }] : undefined,
  });
  if (!result) return null;
  return Array.isArray(result) ? result[0] as string : result as string;
}

export async function saveOutputFileDialog(
  defaultName: string,
  defaultPath?: string,
  isSrtOnly = false,
): Promise<string | null> {
  const path = defaultPath ? `${defaultPath}/${defaultName}` : defaultName;
  const filters = isSrtOnly
    ? [
        { name: 'SubRip Subtitles', extensions: ['srt'] },
        { name: 'All Files', extensions: ['*'] },
      ]
    : [
        { name: 'MP4 Video', extensions: ['mp4'] },
        { name: 'MKV Video', extensions: ['mkv'] },
        { name: 'MOV Video', extensions: ['mov'] },
        { name: 'All Files', extensions: ['*'] },
      ];
  const result = await saveDialog({
    defaultPath: path,
    filters,
  });
  return result ?? null;
}

