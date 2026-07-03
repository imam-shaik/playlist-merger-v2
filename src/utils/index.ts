import type { MediaInfo, PlaylistEntry, IssueKind } from '@/types';
import { convertFileSrc } from '@tauri-apps/api/core';

// ─── Formatting ──────────────────────────────────────────────────────────────

export function formatDuration(seconds: number): string {
  if (!isFinite(seconds) || seconds < 0) return '—';
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

export function formatBytes(bytes: number, decimals = 1): string {
  if (!bytes || bytes <= 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(decimals))} ${sizes[Math.min(i, sizes.length - 1)]}`;
}

export function formatFps(fps: number | undefined): string {
  if (fps === undefined || fps === null) return '—';
  return Number.isInteger(fps) ? String(fps) : fps.toFixed(2);
}

export function formatResolution(info: MediaInfo | null): string {
  const v = info?.videoStreams?.[0];
  if (!v?.width || !v?.height) return '—';
  return `${v.width}×${v.height}`;
}

export function formatEta(seconds: number | undefined): string {
  if (seconds === undefined || !isFinite(seconds) || seconds < 0) return '';
  if (seconds < 5) return '< 5s';
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  if (m > 0) return `~${m}m ${s}s`;
  return `~${s}s`;
}

export function formatSpeed(speed: number | undefined): string {
  if (!speed || speed <= 0) return '';
  return `${speed.toFixed(1)}×`;
}

// ─── Validation ──────────────────────────────────────────────────────────────

export function sanitizeFilename(name: string): string {
  // eslint-disable-next-line no-control-regex
  const invalidChars = /[<>:"/\\|?*\x00-\x1f]/g;
  return name
    .replace(invalidChars, '_')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/[. ]+$/, '')
    .substring(0, 200) || 'output';
}

export function ensureVideoExtension(path: string, preferred = 'mp4'): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  const videoExts = ['mp4', 'mkv', 'mov', 'avi', 'webm', 'm4v'];
  return videoExts.includes(ext) ? path : `${path}.${preferred}`;
}

export function forceVideoExtension(path: string, preferred = 'mp4'): string {
  const lastDot = path.lastIndexOf('.');
  if (lastDot === -1) return `${path}.${preferred}`;
  const ext = path.slice(lastDot + 1).toLowerCase();
  const videoExts = ['mp4', 'mkv', 'mov', 'avi', 'webm', 'm4v'];
  if (videoExts.includes(ext)) return path.slice(0, lastDot) + '.' + preferred;
  return `${path}.${preferred}`;
}

export function estimateOutputSize(entries: PlaylistEntry[]): number {
  return entries.reduce((t, e) => t + (e.size || 0), 0);
}

export function totalDuration(entries: PlaylistEntry[]): number {
  return entries.reduce((t, e) => t + (e.mediaInfo?.duration ?? 0), 0);
}

// ─── Compatibility breakdown ─────────────────────────────────────────────────

export type BreakdownStatus = 'ok' | 'warning' | 'error';

export interface CompatibilityBreakdownItem {
  label: string;
  values: string[];
  status: BreakdownStatus;
}

export function computeCompatibilityBreakdown(
  entries: PlaylistEntry[],
  issueCounts: Record<string, number> = {},
): CompatibilityBreakdownItem[] {
  const items: CompatibilityBreakdownItem[] = [];
  const withInfo = entries.filter((e) => e.mediaInfo);
  if (withInfo.length === 0) return items;

  const distinct = <T>(arr: T[]): T[] => [...new Set(arr)];

  const videoCodecs = distinct(
    withInfo.map((e) => e.mediaInfo!.videoStreams?.[0]?.codecName ?? '').filter(Boolean),
  );
  const audioCodecs = distinct(
    withInfo.map((e) => e.mediaInfo!.audioStreams?.[0]?.codecName ?? '').filter(Boolean),
  );
  const resolutions = distinct(
    withInfo
      .map((e) => {
        const v = e.mediaInfo!.videoStreams?.[0];
        return v?.width && v?.height ? `${v.width}×${v.height}` : '';
      })
      .filter(Boolean),
  );
  const fpsValues = (distinct(
    withInfo
      .map((e) => {
        const v = e.mediaInfo!.videoStreams?.[0];
        return v?.fps ? String(Math.round(v.fps)) : '';
      })
      .filter(Boolean) as string[],
  ));
  const sampleRates = (distinct(
    withInfo.map((e) => String(e.mediaInfo!.audioStreams?.[0]?.sampleRate ?? '')).filter(Boolean) as string[],
  ));
  const channels = distinct(
    withInfo
      .map((e) => e.mediaInfo!.audioStreams?.[0]?.channels ?? 0)
      .filter((c) => c > 0)
      .map(String),
  );

  const status = (key: string): BreakdownStatus => {
    const count = issueCounts[key] ?? 0;
    if (count === 0) return 'ok';
    return 'error';
  };

  if (videoCodecs.length > 0)
    items.push({ label: 'Video', values: videoCodecs, status: status('videoCodecMismatch') });
  if (audioCodecs.length > 0)
    items.push({ label: 'Audio', values: audioCodecs, status: status('audioCodecMismatch') });
  if (resolutions.length > 0)
    items.push({ label: 'Resolution', values: resolutions, status: status('resolutionMismatch') });
  if (fpsValues.length > 0)
    items.push({ label: 'FPS', values: fpsValues, status: status('fpsMismatch') });
  if (sampleRates.length > 0)
    items.push({ label: 'Sample Rate', values: sampleRates, status: status('sampleRateMismatch') });
  if (channels.length > 0)
    items.push({ label: 'Channels', values: channels, status: status('channelMismatch') });

  return items;
}

// ─── Issue labels ─────────────────────────────────────────────────────────────

export function issueKindLabel(kind: IssueKind): string {
  const labels: Record<IssueKind, string> = {
    videoCodecMismatch: 'Video Codec Mismatch',
    audioCodecMismatch: 'Audio Codec Mismatch',
    subtitleCodecMismatch: 'Subtitle Mismatch',
    resolutionMismatch: 'Resolution Mismatch',
    fpsMismatch: 'Frame Rate Mismatch',
    pixelFormatMismatch: 'Pixel Format Mismatch',
    sampleRateMismatch: 'Sample Rate Mismatch',
    channelMismatch: 'Audio Channel Mismatch',
    containerMismatch: 'Container Format Mismatch',
    timebaseMismatch: 'Timebase Mismatch',
  };
  return labels[kind] ?? kind;
}

// ─── ID generation ───────────────────────────────────────────────────────────

let _counter = 0;
export function generateId(): string {
  return `${Date.now().toString(36)}-${(++_counter).toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
}

// ─── Array utilities ─────────────────────────────────────────────────────────

export function moveItems<T>(arr: T[], fromIndex: number, toIndex: number): T[] {
  const result = [...arr];
  const [item] = result.splice(fromIndex, 1);
  result.splice(toIndex, 0, item);
  return result;
}

// ─── Path utilities ──────────────────────────────────────────────────────────

export function getFilename(path: string): string {
  return path.replace(/\\/g, '/').split('/').pop() ?? path;
}

export function getBasename(path: string): string {
  const filename = getFilename(path);
  const dot = filename.lastIndexOf('.');
  return dot > 0 ? filename.slice(0, dot) : filename;
}

export function getExtension(path: string): string {
  return path.split('.').pop()?.toLowerCase() ?? '';
}

export function getDirectory(path: string): string {
  const normalized = path.replace(/\\/g, '/');
  const idx = normalized.lastIndexOf('/');
  return idx > 0 ? normalized.substring(0, idx) : normalized;
}

/** Convert a native file path to a Tauri asset URL for display in <img> */
export function pathToAssetUrl(filePath: string): string {
  if (!filePath) return '';
  // Use Tauri v2's convertFileSrc for proper asset protocol handling
  const url = convertFileSrc(filePath);
  console.debug('[Thumb] pathToAssetUrl:', { input: filePath, output: url });
  return url;
}

// ─── Color utilities for canvas overlay cards ────────────────────────────────

/**
 * Compute relative luminance of a hex color using the WCAG formula.
 * Returns a value between 0 (darkest) and 1 (lightest).
 */
export function getLuminance(hex: string): number {
  const c = hex.replace('#', '');
  if (c.length < 6) return 0.5;
  const r = parseInt(c.slice(0, 2), 16) / 255;
  const g = parseInt(c.slice(2, 4), 16) / 255;
  const b = parseInt(c.slice(4, 6), 16) / 255;
  const rs = r <= 0.03928 ? r / 12.92 : Math.pow((r + 0.055) / 1.055, 2.4);
  const gs = g <= 0.03928 ? g / 12.92 : Math.pow((g + 0.055) / 1.055, 2.4);
  const bs = b <= 0.03928 ? b / 12.92 : Math.pow((b + 0.055) / 1.055, 2.4);
  return 0.2126 * rs + 0.7152 * gs + 0.0722 * bs;
}

const WINDOWS_RESERVED_NAMES = new Set([
  'CON', 'PRN', 'AUX', 'NUL',
  'COM1', 'COM2', 'COM3', 'COM4', 'COM5', 'COM6', 'COM7', 'COM8', 'COM9',
  'LPT1', 'LPT2', 'LPT3', 'LPT4', 'LPT5', 'LPT6', 'LPT7', 'LPT8', 'LPT9',
]);

// eslint-disable-next-line no-control-regex
const INVALID_FILENAME_CHARS = /[<>:"|?*\\/\x00-\x1f]/;
const MAX_FILENAME_LENGTH = 255;

export function validateFilename(filename: string): string | null {
  const trimmed = filename.trim();

  if (!trimmed) {
    return 'Filename cannot be empty';
  }

  if (trimmed.length > MAX_FILENAME_LENGTH) {
    return `Filename is too long (max ${MAX_FILENAME_LENGTH} characters)`;
  }

  if (INVALID_FILENAME_CHARS.test(trimmed)) {
    return 'Filename contains invalid characters: < > : " | ? * \\ /';
  }

  const upper = trimmed.toUpperCase();
  if (WINDOWS_RESERVED_NAMES.has(upper) || WINDOWS_RESERVED_NAMES.has(upper.split('.')[0])) {
    return `Filename "${trimmed}" is a reserved Windows name`;
  }

  if (trimmed.endsWith('.') || trimmed.endsWith(' ')) {
    return 'Filename cannot end with a period or space';
  }

  return null;
}

/**
 * Return a contrasting font color (black or white) based on background luminance.
 * Uses the WCAG contrast threshold of ~0.179.
 */
export function getContrastFontColor(hex: string): string {
  return getLuminance(hex) > 0.179 ? '#000000' : '#FFFFFF';
}
