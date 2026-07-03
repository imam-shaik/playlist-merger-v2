import React, { useEffect, useState } from 'react';
import { FolderOpen, Terminal, CheckCircle2, XCircle, Save, RotateCcw } from 'lucide-react';
import { cn } from '@/utils/cn';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Separator } from '@/components/ui/Badge';
import { useAppStore } from '@/store/appStore';
import { useMergeStore } from '@/store/mergeStore';
import { tauriCommands, openFolderDialog, openExecutableDialog } from '@/tauri/commands';
import type { AppSettings, FfmpegPaths, LargePlaylistStrategy, MergeMode } from '@/types';

export function SettingsScreen() {
  const appStore = useAppStore();
  const mergeStore = useMergeStore();
  const [settings, setSettings] = useState<AppSettings | null>(appStore.settings);
  const [ffmpegPaths, setFfmpegPaths] = useState<FfmpegPaths | null>(appStore.ffmpegPaths);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    tauriCommands.getFfmpegPath().then(setFfmpegPaths).catch(console.error);
  }, []);

  const update = (patch: Partial<AppSettings>) => {
    setSettings(s => s ? { ...s, ...patch } : s);
  };

  const save = async () => {
    if (!settings) return;
    setSaving(true);
    try {
      await tauriCommands.saveSettings(settings);
      appStore.setSettings(settings);
      // Apply defaultMergeMode to mergeStore immediately
      if (settings.defaultMergeMode) {
        mergeStore.setMergeMode(settings.defaultMergeMode);
      }
      const paths = await tauriCommands.getFfmpegPath();
      setFfmpegPaths(paths);
      appStore.setFfmpegPaths(paths);
      appStore.showToast({ type: 'success', title: 'Settings saved' });
    } catch (err) {
      appStore.showToast({ type: 'error', title: 'Failed to save', description: String(err) });
    } finally {
      setSaving(false);
    }
  };

  const resetToDefaults = async () => {
    const defaults: AppSettings = {
      ffmpegPath: undefined,
      ffprobePath: undefined,
      lastExportDir: undefined,
      thumbnailCacheDir: undefined,
      maxThumbnailCacheMb: 500,
      recentExports: [],
      defaultMergeMode: 'lossless',
      checkCompatBeforeMerge: true,
      autoSavePlaylist: true,
    };
    setSettings(defaults);
    try {
      await tauriCommands.saveSettings(defaults);
      appStore.setSettings(defaults);
      mergeStore.setMergeMode('lossless');
      // Refresh ffmpeg path detection so UI reflects cleared paths
      const paths = await tauriCommands.getFfmpegPath();
      setFfmpegPaths(paths);
      appStore.setFfmpegPaths(paths);
      appStore.showToast({ type: 'success', title: 'Settings reset to defaults' });
    } catch (err) {
      appStore.showToast({ type: 'error', title: 'Failed to reset', description: String(err) });
    }
  };

  if (!settings) return null;

  return (
    <div className="flex flex-col h-full overflow-y-auto">
      <div className="p-4 flex flex-col gap-6">

        {/* FFmpeg Paths */}
        <section>
          <h3 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-3 flex items-center gap-2">
            <Terminal className="w-3.5 h-3.5" />
            FFmpeg Binaries
          </h3>

          {ffmpegPaths && (
            <div className="flex items-center gap-2 mb-3 p-2.5 rounded-lg bg-bg-elevated border border-border">
              <div className="flex items-center gap-1.5">
                {ffmpegPaths.ffmpegFound
                  ? <CheckCircle2 className="w-3.5 h-3.5 text-success" />
                  : <XCircle className="w-3.5 h-3.5 text-danger" />}
                <span className="text-xs text-text-secondary">
                  ffmpeg: {ffmpegPaths.ffmpeg
                    ? <span className="font-mono text-text-muted">{ffmpegPaths.ffmpeg}</span>
                    : 'Not found'}
                </span>
              </div>
            </div>
          )}

          <div className="flex flex-col gap-2">
            <Input
              label="FFmpeg path (leave blank for auto-detect)"
              value={settings.ffmpegPath ?? ''}
              onChange={(e) => update({ ffmpegPath: e.target.value || undefined })}
              placeholder={ffmpegPaths?.ffmpeg || 'Auto-detect (e.g. C:\\path\\to\\ffmpeg.exe)'}
              rightElement={
                <button
                  onClick={async () => {
                    const p = await openExecutableDialog('Select FFmpeg Binary');
                    if (p) update({ ffmpegPath: p });
                  }}
                  className="text-text-muted hover:text-text-secondary transition-colors"
                >
                  <FolderOpen className="w-3.5 h-3.5" />
                </button>
              }
            />
            <Input
              label="FFprobe path (leave blank for auto-detect)"
              value={settings.ffprobePath ?? ''}
              onChange={(e) => update({ ffprobePath: e.target.value || undefined })}
              placeholder={ffmpegPaths?.ffprobe || 'Auto-detect (e.g. C:\\path\\to\\ffprobe.exe)'}
              rightElement={
                <button
                  onClick={async () => {
                    const p = await openExecutableDialog('Select FFprobe Binary');
                    if (p) update({ ffprobePath: p });
                  }}
                  className="text-text-muted hover:text-text-secondary transition-colors"
                >
                  <FolderOpen className="w-3.5 h-3.5" />
                </button>
              }
            />
          </div>
        </section>

        <Separator />

        {/* Export */}
        <section>
          <h3 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-3">
            Export Defaults
          </h3>
          <Input
            label="Default export directory"
            value={settings.lastExportDir ?? ''}
            onChange={(e) => update({ lastExportDir: e.target.value })}
            placeholder="Last used location"
            rightElement={
              <button
                onClick={async () => {
                  const p = await openFolderDialog();
                  if (p) update({ lastExportDir: p });
                }}
                className="text-text-muted hover:text-text-secondary transition-colors"
              >
                <FolderOpen className="w-3.5 h-3.5" />
              </button>
            }
          />
        </section>

        <Separator />

        {/* Default Merge Mode */}
        <section>
          <h3 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-3">
            Default Merge Mode
          </h3>
          <p className="text-xs text-text-muted mb-2">
            Choose the merge mode that is pre-selected when opening the merge panel.
          </p>
          <div className="flex flex-col gap-1.5">
            {([
              { value: 'lossless' as MergeMode, label: 'Lossless', desc: 'Stream copy — fastest, no quality loss' },
              { value: 'smartMkv' as MergeMode, label: 'Smart MKV', desc: 'Intelligent re-encode only when needed' },
              { value: 'fastMkv' as MergeMode, label: 'Fast MKV', desc: 'Lossless concat with normalization' },
              { value: 'custom' as MergeMode, label: 'Custom', desc: 'Full re-encode with manual settings' },
            ] as const).map(opt => (
              <button
                key={opt.value}
                onClick={() => update({ defaultMergeMode: opt.value })}
                className={cn(
                  'flex items-center justify-between p-3 rounded-lg border text-left transition-all',
                  settings.defaultMergeMode === opt.value
                    ? 'border-accent-500 bg-accent-500/10'
                    : 'border-border bg-bg-elevated hover:border-border-strong'
                )}
              >
                <div>
                  <p className="text-sm font-medium text-text-primary">{opt.label}</p>
                  <p className="text-xs text-text-muted mt-0.5">{opt.desc}</p>
                </div>
                <div className={cn(
                  'w-4 h-4 rounded-full border-2 shrink-0 flex items-center justify-center',
                  settings.defaultMergeMode === opt.value
                    ? 'border-accent-500 bg-accent-500'
                    : 'border-border'
                )}>
                  {settings.defaultMergeMode === opt.value && (
                    <div className="w-1.5 h-1.5 rounded-full bg-white" />
                  )}
                </div>
              </button>
            ))}
          </div>
        </section>

        <Separator />

        {/* Behavior */}
        <section>
          <h3 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-3">
            Behavior
          </h3>
          <div className="flex flex-col gap-2">
            {([
              { key: 'checkCompatBeforeMerge' as const, label: 'Check compatibility before merge', desc: 'Probe all files and warn about issues' },
              { key: 'autoSavePlaylist' as const, label: 'Auto-save playlist', desc: 'Automatically save playlist changes' },
            ] as const).map(opt => (
              <button
                key={opt.key}
                onClick={() => update({ [opt.key]: !settings[opt.key] })}
                className="flex items-center justify-between p-3 rounded-lg bg-bg-elevated border border-border hover:border-border-strong transition-colors text-left"
              >
                <div>
                  <p className="text-sm text-text-primary">{opt.label}</p>
                  <p className="text-xs text-text-muted mt-0.5">{opt.desc}</p>
                </div>
                <div className={cn(
                  'w-9 h-5 rounded-full transition-colors duration-200 flex items-center px-0.5 shrink-0',
                  settings[opt.key] ? 'bg-accent-600' : 'bg-bg-overlay border border-border'
                )}>
                  <div className={cn(
                    'w-4 h-4 rounded-full bg-white transition-transform duration-200',
                    settings[opt.key] ? 'translate-x-4' : 'translate-x-0'
                  )} />
                </div>
              </button>
            ))}
          </div>
        </section>

        {/* Large Playlist */}
        <section>
          <h3 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-3">
            Large Playlist
          </h3>
          <div className="flex flex-col gap-1.5">
            <p className="text-xs text-text-muted mb-2">
              When Smart mode is used with 60+ files, choose the default audio validation strategy. Selecting a default skips the merge dialog.
            </p>
            {([
              { value: undefined as LargePlaylistStrategy | undefined, label: 'Always ask', desc: 'Show dialog before each large merge' },
              { value: 'fullSmart' as LargePlaylistStrategy, label: 'Full Smart', desc: 'Thorough probe & repair — longest pre-merge' },
              { value: 'smartLite' as LargePlaylistStrategy, label: 'SmartLite', desc: 'Targeted probe — recommended balance' },
              { value: 'safe' as LargePlaylistStrategy, label: 'Safe', desc: 'Skip audio probing — fastest pre-merge' },
              { value: 'fast' as LargePlaylistStrategy, label: 'Fast', desc: 'Minimal checks only — no audio validation' },
            ] as const).map(opt => (
              <button
                key={String(opt.value)}
                onClick={() => update({ largePlaylistDefault: opt.value })}
                className={cn(
                  'flex items-center justify-between p-3 rounded-lg border text-left transition-all',
                  settings.largePlaylistDefault === opt.value
                    ? 'border-accent-500 bg-accent-500/10'
                    : 'border-border bg-bg-elevated hover:border-border-strong'
                )}
              >
                <div>
                  <p className="text-sm font-medium text-text-primary">{opt.label}</p>
                  <p className="text-xs text-text-muted mt-0.5">{opt.desc}</p>
                </div>
                <div className={cn(
                  'w-4 h-4 rounded-full border-2 shrink-0 flex items-center justify-center',
                  settings.largePlaylistDefault === opt.value
                    ? 'border-accent-500 bg-accent-500'
                    : 'border-border'
                )}>
                  {settings.largePlaylistDefault === opt.value && (
                    <div className="w-1.5 h-1.5 rounded-full bg-white" />
                  )}
                </div>
              </button>
            ))}
          </div>
        </section>

        {/* Thumbnail cache */}
        <section>
          <h3 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-3">
            Thumbnails
          </h3>
          <Input
            label="Max thumbnail cache (MB)"
            value={String(settings.maxThumbnailCacheMb)}
            onChange={(e) => update({ maxThumbnailCacheMb: Number(e.target.value) })}
            type="number"
            min={50}
            max={5000}
          />
        </section>
      </div>

      {/* Save / Reset */}
      <div className="sticky bottom-0 p-4 bg-bg-surface/80 backdrop-blur-sm border-t border-border mt-auto flex gap-2">
        <Button
          variant="primary"
          size="lg"
          className="flex-1"
          leftIcon={<Save className="w-4 h-4" />}
          loading={saving}
          onClick={save}
        >
          Save Settings
        </Button>
        <Button
          variant="danger"
          size="lg"
          leftIcon={<RotateCcw className="w-4 h-4" />}
          onClick={resetToDefaults}
        >
          Reset
        </Button>
      </div>
    </div>
  );
}
