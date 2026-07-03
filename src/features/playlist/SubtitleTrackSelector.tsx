import React from 'react';
import { useMergeStore } from '@/store/mergeStore';
import { usePlaylistStore } from '@/store/playlistStore';
import { cn } from '@/utils/cn';
import type { SubtitleStream } from '@/types';

/**
 * Language display helper — returns a clean label for a subtitle stream.
 */
function streamLabel(stream: SubtitleStream, idx: number): string {
  const lang = stream.language?.toUpperCase();
  const title = stream.title;
  const codec = stream.codecName.toUpperCase();
  const trackLabel = `Track ${idx + 1}`;

  if (lang && title) return `${lang} — ${title}`;
  if (lang) return `${lang} (${codec})`;
  if (title) return `${title} (${codec})`;
  return `${trackLabel} — ${codec}`;
}

/**
 * Stream sub-label (language + stream index for tooltips).
 */
function streamSublabel(stream: SubtitleStream): string {
  const parts: string[] = [];
  if (stream.language) parts.push(`lang: ${stream.language}`);
  parts.push(`stream #${stream.streamIndex}`);
  if (stream.codecLongName && stream.codecLongName !== stream.codecName) {
    parts.push(stream.codecLongName);
  }
  return parts.join(' · ');
}

export function SubtitleTrackSelector() {
  const entries = usePlaylistStore((s) => s.entries);
  const subtitleMode = useMergeStore((s) => s.subtitleMode);
  const selectedIndices = useMergeStore((s) => s.selectedSubtitleStreamIndices);
  const setSelected = useMergeStore((s) => s.setSelectedSubtitleStreamIndex);

  // Only show when subtitle processing is enabled
  if (subtitleMode === 'none' || subtitleMode === 'exportSrt') return null;

  // Filter to entries that have embedded subtitle streams
  const entriesWithSubs = entries.filter(
    (e) => {
      const embStreams = e.mediaInfo?.subtitleStreams?.filter((s) => !s.isExternal);
      return embStreams && embStreams.length > 1; // Only show if 2+ embedded tracks
    }
  );

  if (entriesWithSubs.length === 0) return null;

  const handleSelect = (entryId: string, streamIndex: number | null) => {
    setSelected(entryId, streamIndex);
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <svg className="w-3.5 h-3.5 text-accent-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M2 7a2 2 0 012-2h16a2 2 0 012 2v10a2 2 0 01-2 2H4a2 2 0 01-2-2V7z" />
          <path d="M6 12h2m4 0h6m-8 4h8" />
        </svg>
        <span className="text-[10px] font-semibold text-text-muted uppercase tracking-wider">
          Embedded Subtitle Track Selection
        </span>
      </div>

      <p className="text-[9px] text-text-muted leading-tight -mt-1">
        Some files contain multiple embedded subtitle tracks. Choose which track to use per file, or let the app auto-select the first track.
      </p>

      <div className="flex flex-col gap-1.5">
        {entriesWithSubs.map((entry) => {
          const embStreams = (entry.mediaInfo?.subtitleStreams ?? []).filter((s) => !s.isExternal);
          const currentSelection = selectedIndices[entry.id] ?? null;

          return (
            <div
              key={entry.id}
              className="flex items-start gap-2 px-3 py-2 rounded-lg border border-border/40 bg-bg-surface/20"
            >
              {/* File name */}
              <div className="flex-1 min-w-0">
                <span className="text-[10px] font-medium text-text-primary truncate block">
                  {entry.name}
                </span>
                <span className="text-[8px] text-text-muted">
                  {embStreams.length} subtitle tracks
                </span>
              </div>

              {/* Track selector */}
              <div className="flex flex-col gap-0.5 min-w-[140px]">
                {embStreams.map((stream, streamIdx) => {
                  const isSelected = currentSelection === stream.streamIndex;
                  const isAuto = currentSelection === null && streamIdx === 0;
                  const active = isSelected || isAuto;

                  return (
                    <button
                      key={stream.streamIndex}
                      type="button"
                      onClick={() => handleSelect(entry.id, stream.streamIndex)}
                      className={cn(
                        'flex items-center gap-2 px-2 py-1 rounded-md text-left transition-all',
                        active
                          ? 'bg-accent-500/10 border border-accent-500/30'
                          : 'bg-transparent border border-transparent hover:bg-bg-overlay/50'
                      )}
                      title={streamSublabel(stream)}
                    >
                      {/* Radio indicator */}
                      <span className={cn(
                        'w-2.5 h-2.5 rounded-full border-2 shrink-0 flex items-center justify-center transition-colors',
                        active
                          ? 'border-accent-500 bg-accent-500'
                          : 'border-text-disabled'
                      )}>
                        {active && <span className="w-1 h-1 rounded-full bg-white" />}
                      </span>

                      {/* Stream label */}
                      <div className="flex flex-col min-w-0">
                        <span className={cn(
                          'text-[9px] font-medium truncate',
                          active ? 'text-accent-400' : 'text-text-secondary'
                        )}>
                          {streamLabel(stream, streamIdx)}
                          {isAuto && (
                            <span className="ml-1 text-[7px] text-accent-500/70 font-normal">
                              auto
                            </span>
                          )}
                        </span>
                        {stream.language && (
                          <span className="text-[7px] text-text-muted truncate">
                            {streamSublabel(stream)}
                          </span>
                        )}
                      </div>
                    </button>
                  );
                })}

                {/* Option to reset to auto */}
                {currentSelection !== null && (
                  <button
                    type="button"
                    onClick={() => handleSelect(entry.id, null)}
                    className="flex items-center gap-2 px-2 py-0.5 rounded-md text-left transition-all hover:bg-bg-overlay/50"
                  >
                    <span className="w-2.5 h-2.5 flex items-center justify-center">
                      <span className="w-1.5 h-0.5 rounded-full bg-text-disabled" />
                    </span>
                    <span className="text-[8px] text-text-muted hover:text-text-secondary transition-colors">
                      Auto (first track)
                    </span>
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
