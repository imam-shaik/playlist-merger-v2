// ────────────────────────────────────────────────
// EncodingSettings — Custom mode video/audio
// encoding options with collapsible advanced section.
// ────────────────────────────────────────────────

import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { cn } from '@/utils/cn';
import { Input } from '@/components/ui/Input';
import { useMergeStore } from '@/store/mergeStore';
import { useWorkspaceStore } from '@/store/workspaceStore';
import {
  VIDEO_CODECS, AUDIO_CODECS, ENCODER_PRESETS,
  AUDIO_BITRATES, RESOLUTION_PRESETS, FPS_PRESETS,
} from '@/constants';

// ─── Select component ────────────────────────────

interface SelectProps {
  label?: string;
  value: string;
  onChange: (v: string) => void;
  options: readonly { value: string; label: string }[];
  className?: string;
}

function Select({ label, value, onChange, options, className }: SelectProps) {
  const id = label?.toLowerCase().replace(/\s+/g, '-');
  return (
    <div className={cn('flex flex-col gap-1.5', className)}>
      {label && (
        <label htmlFor={id} className="text-xs font-medium text-text-secondary">{label}</label>
      )}
      <select
        id={id}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className={cn(
          'w-full h-8 px-3 text-sm appearance-none',
          'bg-bg-surface border border-border rounded',
          'text-text-primary',
          'focus:outline-none focus:ring-2 focus:ring-accent-500/40 focus:border-accent-500/60',
          'hover:border-border-strong transition-colors',
          'cursor-pointer',
        )}
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>{o.label}</option>
        ))}
      </select>
    </div>
  );
}

// ─── Main Component ──────────────────────────────

export function EncodingSettings() {
  const mergeMode = useMergeStore((s) => s.mergeMode);
  const videoCodec = useMergeStore((s) => s.videoCodec);
  const videoPreset = useMergeStore((s) => s.videoPreset);
  const videoCrf = useMergeStore((s) => s.videoCrf);
  const audioCodec = useMergeStore((s) => s.audioCodec);
  const audioBitrate = useMergeStore((s) => s.audioBitrate);
  const targetResolution = useMergeStore((s) => s.targetResolution);
  const targetFps = useMergeStore((s) => s.targetFps);
  const hwAccel = useMergeStore((s) => s.hwAccel);
  const advancedExpanded = useWorkspaceStore((s) => s.sectionsCollapsed.advanced);
  const toggleSection = useWorkspaceStore((s) => s.toggleSection);
  const setSectionCollapsed = useWorkspaceStore((s) => s.setSectionCollapsed);

  // Always expand advanced when custom mode is selected
  React.useEffect(() => {
    if (mergeMode === 'custom') {
      setSectionCollapsed('encoding', true);
    }
  }, [mergeMode, setSectionCollapsed]);

  // When custom mode is deselected, collapse everything
  if (mergeMode !== 'custom') return null;

  // Get setters imperatively (stable references, no re-render needed)
  const set = useMergeStore.getState();

  return (
    <motion.div
      initial={{ opacity: 0, height: 0 }}
      animate={{ opacity: 1, height: 'auto' }}
      exit={{ opacity: 0, height: 0 }}
      transition={{ duration: 0.2 }}
      className="overflow-hidden"
    >
      <div className="bg-bg-elevated border border-border rounded-xl p-4 flex flex-col gap-3">
        <p className="text-xs font-semibold text-text-muted uppercase tracking-wider">
          Encoding Settings
        </p>

        {/* Video codec + preset */}
        <div className="grid grid-cols-2 gap-2">
          <Select
            label="Video Codec"
            value={videoCodec}
            onChange={set.setVideoCodec}
            options={VIDEO_CODECS}
          />
          <Select
            label="Encoder Preset"
            value={videoPreset}
            onChange={set.setVideoPreset}
            options={ENCODER_PRESETS}
          />
        </div>

        {/* CRF slider */}
        <div>
          <div className="flex items-center justify-between mb-1.5">
            <label className="text-xs font-medium text-text-secondary">
              Quality (CRF)
            </label>
            <output className="text-xs font-mono text-text-primary">{videoCrf}</output>
          </div>
          <input
            type="range"
            min={0} max={51} step={1}
            value={videoCrf}
            onChange={(e) => set.setVideoCrf(Number(e.target.value))}
            className="w-full h-1.5 appearance-none rounded bg-border cursor-pointer accent-accent-500"
            aria-label="Quality CRF value"
          />
          <div className="flex justify-between mt-1">
            <span className="text-2xs text-text-muted">0 = Lossless</span>
            <span className="text-2xs text-text-muted">51 = Worst</span>
          </div>
        </div>

        {/* Audio */}
        <div className="grid grid-cols-2 gap-2">
          <Select
            label="Audio Codec"
            value={audioCodec}
            onChange={set.setAudioCodec}
            options={AUDIO_CODECS}
          />
          <Select
            label="Audio Bitrate"
            value={audioBitrate}
            onChange={set.setAudioBitrate}
            options={AUDIO_BITRATES}
          />
        </div>

        {/* Advanced toggle */}
        <button
          onClick={() => toggleSection('advanced')}
          className="flex items-center gap-1.5 text-xs text-text-muted hover:text-text-secondary transition-colors"
          aria-expanded={advancedExpanded}
          aria-label="Toggle advanced options"
        >
          {advancedExpanded ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
          Advanced Options
        </button>

        <AnimatePresence>
          {advancedExpanded && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: 'auto' }}
              exit={{ opacity: 0, height: 0 }}
              className="overflow-hidden"
            >
              <div className="flex flex-col gap-2 pt-1">
                <div className="grid grid-cols-2 gap-2">
                  <Select
                    label="Resolution"
                    value={targetResolution}
                    onChange={set.setTargetResolution}
                    options={RESOLUTION_PRESETS}
                  />
                  <Select
                    label="Frame Rate"
                    value={targetFps}
                    onChange={set.setTargetFps}
                    options={FPS_PRESETS}
                  />
                </div>
                <Input
                  label="Hardware Acceleration (optional)"
                  value={hwAccel}
                  onChange={(e) => set.setHwAccel(e.target.value)}
                  placeholder="e.g. cuda, dxva2, d3d11va"
                />
                <p className="text-2xs text-text-muted">
                  GPU acceleration requires compatible hardware and drivers. Leave blank for CPU encoding.
                </p>
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </motion.div>
  );
}
