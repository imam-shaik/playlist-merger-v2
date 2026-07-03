import type { NamingMode } from '@/types';

export const SUPPORTED_VIDEO_EXTENSIONS = [
  'mp4', 'mkv', 'mov', 'avi', 'webm', 'm4v', 'ts',
  'mts', 'm2ts', 'flv', 'wmv', '3gp', 'ogv',
] as const;

export type SupportedExtension = typeof SUPPORTED_VIDEO_EXTENSIONS[number];

export const THUMBNAIL = {
  WIDTH: 160,
  HEIGHT: 90,
  DEFAULT_SEEK_SECONDS: 2,
  MAX_CONCURRENT: 3,
} as const;

export const PLAYLIST_ITEM = {
  HEIGHT: 72,
  OVERSCAN_COUNT: 5,
} as const;

export const MERGE_DEFAULTS = {
  VIDEO_CODEC: 'libx264',
  AUDIO_CODEC: 'aac',
  VIDEO_CRF: 18,
  VIDEO_PRESET: 'medium',
  AUDIO_BITRATE: '192k',
} as const;

export const PROBE = {
  MAX_CONCURRENT: 6,
  AUTO_PROBE_THRESHOLD: 50,
} as const;

export const ANIMATION = {
  FAST: 150,
  DEFAULT: 250,
  SLOW: 400,
} as const;

export const OUTPUT = {
  DEFAULT_FILENAME: 'merged_output',
} as const;

export const AUTOSAVE_DEBOUNCE_MS = 2000;

export const DISK = {
  LOW_SPACE_BYTES: 5 * 1024 * 1024 * 1024,
  SAFETY_MULTIPLIER: 1.2,
} as const;

export const HISTORY = {
  MAX_RECENT_EXPORTS: 10,
} as const;

export const VIDEO_CODECS = [
  { value: 'libx264', label: 'H.264 (libx264)', supportsPreset: true },
  { value: 'libx265', label: 'H.265 / HEVC (libx265)', supportsPreset: true },
  { value: 'libsvtav1', label: 'AV1 (SVT-AV1)', supportsPreset: true },
  { value: 'libvpx-vp9', label: 'VP9 (libvpx)', supportsPreset: false },
  { value: 'h264_nvenc', label: 'H.264 NVENC (GPU)', supportsPreset: false },
  { value: 'hevc_nvenc', label: 'H.265 NVENC (GPU)', supportsPreset: false },
  { value: 'h264_qsv', label: 'H.264 Intel QSV (GPU)', supportsPreset: false },
  { value: 'h264_amf', label: 'H.264 AMD AMF (GPU)', supportsPreset: false },
] as const;

export const AUDIO_CODECS = [
  { value: 'aac', label: 'AAC' },
  { value: 'libopus', label: 'Opus' },
  { value: 'mp3', label: 'MP3 (libmp3lame)' },
  { value: 'ac3', label: 'AC3 (Dolby)' },
  { value: 'copy', label: 'Copy Original (passthrough)' },
] as const;

export const ENCODER_PRESETS = [
  { value: 'ultrafast', label: 'Ultra Fast (worst compression)' },
  { value: 'fast', label: 'Fast' },
  { value: 'medium', label: 'Medium (recommended)' },
  { value: 'slow', label: 'Slow (better compression)' },
  { value: 'veryslow', label: 'Very Slow (best compression)' },
] as const;

export const AUDIO_BITRATES = [
  { value: '96k', label: '96 kbps' },
  { value: '128k', label: '128 kbps' },
  { value: '192k', label: '192 kbps (recommended)' },
  { value: '256k', label: '256 kbps' },
  { value: '320k', label: '320 kbps (high quality)' },
] as const;

export const RESOLUTION_PRESETS = [
  { value: '', label: 'Original (keep source)' },
  { value: '3840:2160', label: '4K (3840×2160)' },
  { value: '2560:1440', label: '1440p (2560×1440)' },
  { value: '1920:1080', label: '1080p (1920×1080)' },
  { value: '1280:720', label: '720p (1280×720)' },
  { value: '854:480', label: '480p (854×480)' },
] as const;

export const FPS_PRESETS = [
  { value: '', label: 'Original FPS (keep source)' },
  { value: '60', label: '60 fps' },
  { value: '30', label: '30 fps' },
  { value: '24', label: '24 fps (cinema)' },
  { value: '23.976', label: '23.976 fps' },
] as const;

export const SPLIT_MODES = [
  { value: 'none', label: 'No Split (single file)' },
  { value: 'count', label: 'Split by Part Count' },
  { value: 'duration', label: 'Split by Duration' },
  { value: 'folder', label: 'Split by Folder Group' },
] as const;

export const FOLDER_OUTPUT_MODES = [
  { value: 'single', label: 'Single Output Per Folder' },
  { value: 'parts', label: 'Split Each Folder Into Parts' },
] as const;

export const PART_COUNT_PRESETS = [
  { value: '2', label: '2 parts' },
  { value: '3', label: '3 parts' },
  { value: '4', label: '4 parts' },
  { value: '5', label: '5 parts' },
  { value: '6', label: '6 parts' },
  { value: '8', label: '8 parts' },
  { value: '10', label: '10 parts' },
] as const;

/** Maximum allowed output parts per merge to prevent accidental output explosion */
export const MAX_OUTPUT_PARTS = 500;

// ── Canvas overlay card presets ────────────────────────────────────────────

/** Pre-made canvas color swatches with auto-computed font colors */
export const CANVAS_COLORS = [
  { color: '#3366FF', fontColor: '#FFFFFF', label: 'Blue' },
  { color: '#FF3366', fontColor: '#FFFFFF', label: 'Crimson' },
  { color: '#33CC66', fontColor: '#FFFFFF', label: 'Green' },
  { color: '#FF9933', fontColor: '#FFFFFF', label: 'Orange' },
  { color: '#9933FF', fontColor: '#FFFFFF', label: 'Purple' },
  { color: '#FF3399', fontColor: '#FFFFFF', label: 'Pink' },
  { color: '#00CCCC', fontColor: '#000000', label: 'Teal' },
  { color: '#E6E6E6', fontColor: '#000000', label: 'Light Gray' },
  { color: '#333333', fontColor: '#FFFFFF', label: 'Dark Gray' },
  { color: '#000000', fontColor: '#FFFFFF', label: 'Black' },
] as const;

// ────────────────────────────────────────────────
// Repeat / Extend constants
// ────────────────────────────────────────────────

export const REPEAT_COUNT_PRESETS = [
  { value: '2', label: '×2' },
  { value: '3', label: '×3' },
  { value: '5', label: '×5' },
  { value: '8', label: '×8' },
  { value: '10', label: '×10' },
  { value: '15', label: '×15' },
  { value: '20', label: '×20' },
] as const;

/** Duration presets grouped by unit — value is always in seconds */
export const REPEAT_DURATION_PRESETS_MINUTES = [
  { value: '600', label: '10 min', seconds: 600 },
  { value: '900', label: '15 min', seconds: 900 },
  { value: '1200', label: '20 min', seconds: 1200 },
  { value: '1500', label: '25 min', seconds: 1500 },
  { value: '1800', label: '30 min', seconds: 1800 },
  { value: '2700', label: '45 min', seconds: 2700 },
] as const;

export const REPEAT_DURATION_PRESETS_HOURS = [
  { value: '3600', label: '1 hour', seconds: 3600 },
  { value: '5400', label: '1.5 hours', seconds: 5400 },
  { value: '7200', label: '2 hours', seconds: 7200 },
  { value: '10800', label: '3 hours', seconds: 10800 },
  { value: '14400', label: '4 hours', seconds: 14400 },
] as const;

export const DURATION_PRESETS = [
  { value: '900', label: '15 min (900s)' },
  { value: '1800', label: '30 min (1800s)' },
  { value: '2700', label: '45 min (2700s)' },
  { value: '3600', label: '1 hour (3600s)' },
  { value: '5400', label: '1.5 hours (5400s)' },
  { value: '7200', label: '2 hours (7200s)' },
  { value: '10800', label: '3 hours (10800s)' },
  { value: '14400', label: '4 hours (14400s)' },
] as const;

// ────────────────────────────────────────────────
// Split-specific constants
// ────────────────────────────────────────────────

export const SPLIT_MODES_DEFINITIONS = [
  { value: 'byParts', label: 'By Number of Parts', icon: 'Layers', description: 'Split files evenly across N parts' },
  { value: 'byDuration', label: 'By Duration', icon: 'Clock', description: 'Split into fixed-duration chunks' },
  { value: 'byChapters', label: 'By Chapters', icon: 'BookMarked', description: 'Split at chapter boundaries' },
  { value: 'customRanges', label: 'Custom Ranges', icon: 'SlidersHorizontal', description: 'Define custom start/end ranges' },
  { value: 'byPlaylistItems', label: 'By Playlist Items', icon: 'ListVideo', description: 'Group videos by item count' },
  { value: 'byOutputSize', label: 'By Output Size', icon: 'HardDrive', description: 'Split so each part fits a max size' },
  { value: 'smartCourse', label: 'Smart Course', icon: 'GraduationCap', description: 'Split into daily/weekly learning chunks' },
] as const;

export const PART_COUNT_PRESETS_SPLIT = [
  { value: '2', label: '2 parts' },
  { value: '3', label: '3 parts' },
  { value: '4', label: '4 parts' },
  { value: '5', label: '5 parts' },
  { value: '6', label: '6 parts' },
  { value: '8', label: '8 parts' },
  { value: '10', label: '10 parts' },
  { value: '15', label: '15 parts' },
  { value: '20', label: '20 parts' },
] as const;

export const COURSE_MODE_OPTIONS = [
  { value: 'daily', label: 'Daily Learning' },
  { value: 'weekly', label: 'Weekly Learning' },
] as const;

export const COURSE_HOURS_PRESETS = [
  { value: '1', label: '1 hour' },
  { value: '2', label: '2 hours' },
  { value: '3', label: '3 hours' },
  { value: '4', label: '4 hours' },
  { value: '6', label: '6 hours' },
  { value: '8', label: '8 hours' },
] as const;

export const SPLIT_PART_DURATION_PRESETS = [
  { value: '300', label: '5 min' },
  { value: '600', label: '10 min' },
  { value: '900', label: '15 min' },
  { value: '1800', label: '30 min' },
  { value: '2700', label: '45 min' },
  { value: '3600', label: '1 hour' },
  { value: '5400', label: '1.5 hours' },
  { value: '7200', label: '2 hours' },
  { value: '10800', label: '3 hours' },
] as const;

export const SPLIT_OUTPUT_SIZE_PRESETS = [
  { value: '681574400', label: '650 MB (CD)' },
  { value: '1000000000', label: '1 GB' },
  { value: '2000000000', label: '2 GB' },
  { value: '4000000000', label: '4 GB (FAT32 limit)' },
  { value: '8000000000', label: '8 GB' },
  { value: '16000000000', label: '16 GB' },
] as const;

// ────────────────────────────────────────────────
// Naming system constants
// ────────────────────────────────────────────────

export const NAMING_MODES: NamingMode[] = [
  {
    id: 'sequential',
    label: 'Sequential',
    description: 'Basic numbered parts',
    defaultTemplate: '{filename}_Part_{num3}',
    example: { input: 'Course.mp4', output: 'Course_Part_001.mp4' },
    supportedVariables: ['filename', 'ext', 'num', 'num2', 'num3', 'num4', 'chapter', 'part_label'],
  },
  {
    id: 'prefix',
    label: 'Prefix',
    description: 'Custom prefix followed by number',
    defaultTemplate: '{prefix}_{num3}',
    example: { input: 'Course.mp4', output: 'Day_001.mp4' },
    supportedVariables: ['filename', 'ext', 'num', 'num2', 'num3', 'num4', 'prefix'],
  },
  {
    id: 'suffix',
    label: 'Suffix',
    description: 'Filename with custom suffix and number',
    defaultTemplate: '{filename}_{suffix}_{num3}',
    example: { input: 'Course.mp4', output: 'Course_Completed_001.mp4' },
    supportedVariables: ['filename', 'ext', 'num', 'num2', 'num3', 'num4', 'suffix'],
  },
  {
    id: 'custom',
    label: 'Custom',
    description: 'Full custom template string',
    defaultTemplate: '{filename}_Part_{num3}',
    example: { input: 'Course.mp4', output: 'Course_Part_001.mp4' },
    supportedVariables: ['filename', 'ext', 'num', 'num2', 'num3', 'num4', 'date', 'time', 'start', 'end', 'duration', 'chapter', 'part_label', 'resolution', 'height', 'width', 'folder', 'playlist', 'playlist_index', 'original_num', 'video_count', 'total_duration', 'lang', 'lang_name'],
  },
  {
    id: 'timestamp',
    label: 'Timestamp',
    description: 'Include time ranges in filename',
    defaultTemplate: '{filename}_{start}_{end}',
    example: { input: 'Course.mp4', output: 'Course_00-00-00_00-30-00.mp4' },
    supportedVariables: ['filename', 'ext', 'start', 'end', 'duration', 'chapter'],
  },
  {
    id: 'chapter',
    label: 'Chapter',
    description: 'Chapter-based names with number',
    defaultTemplate: '{chapter}_{num2}',
    example: { input: 'Course.mp4', output: 'Introduction_01.mp4' },
    supportedVariables: ['filename', 'ext', 'num', 'num2', 'num3', 'num4', 'chapter', 'original_num'],
  },
  {
    id: 'playlist',
    label: 'Playlist',
    description: 'Group-based naming for playlists',
    defaultTemplate: '{filename}_{playlist}_{num2}',
    example: { input: 'Course.mp4', output: 'Course_Day_01_01.mp4' },
    supportedVariables: ['filename', 'ext', 'num', 'num2', 'num3', 'num4', 'playlist', 'playlist_index'],
  },
  {
    id: 'date',
    label: 'Date',
    description: 'Include date stamp in filename',
    defaultTemplate: '{filename}_{date}_{num2}',
    example: { input: 'Course.mp4', output: 'Course_2026-05-30_01.mp4' },
    supportedVariables: ['filename', 'ext', 'num', 'num2', 'num3', 'num4', 'date', 'time'],
  },
  {
    id: 'smart_course',
    label: 'Smart Course',
    description: 'Module/chapter pattern for courses',
    defaultTemplate: 'Module_{num2}_{chapter}',
    example: { input: 'Course.mp4', output: 'Module_01_Introduction.mp4' },
    supportedVariables: ['filename', 'ext', 'num', 'num2', 'num3', 'num4', 'chapter', 'part_label'],
  },
];
