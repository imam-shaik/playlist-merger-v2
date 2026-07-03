import React from 'react';

const VARIABLE_GROUPS = [
  {
    label: 'File',
    variables: ['{filename}', '{ext}', '{folder}', '{resolution}', '{width}', '{height}'],
  },
  {
    label: 'Sequence',
    variables: ['{num}', '{num2}', '{num3}', '{num4}', '{original_num}'],
  },
  {
    label: 'Time',
    variables: ['{start}', '{end}', '{duration}', '{total_duration}'],
  },
  {
    label: 'Metadata',
    variables: ['{chapter}', '{part_label}', '{playlist}', '{playlist_index}'],
  },
  {
    label: 'Date/Time',
    variables: ['{date}', '{time}'],
  },
  {
    label: 'Merge',
    variables: ['{video_count}', '{total_duration}'],
  },
  {
    label: 'Language',
    variables: ['{lang}', '{lang_name}'],
  },
];

interface NamingCheatSheetProps {
  onInsert: (variable: string) => void;
}

export function NamingCheatSheet({ onInsert }: NamingCheatSheetProps) {
  return (
    <div className="bg-bg-elevated/40 rounded-lg border border-border/60 p-3 space-y-2">
      <p className="text-[10px] text-text-muted leading-relaxed">
        Click a variable to insert it at the cursor position in the template above.
      </p>
      <div className="grid grid-cols-3 gap-x-4 gap-y-1.5">
        {VARIABLE_GROUPS.map((group) => (
          <div key={group.label}>
            <p className="text-[9px] font-semibold text-text-muted uppercase tracking-wider mb-1">
              {group.label}
            </p>
            <div className="space-y-0.5">
              {group.variables.map((v) => (
                <button
                  key={v}
                  onClick={() => onInsert(v)}
                  className="block text-[10px] text-accent-300 hover:text-accent-200 hover:underline font-mono transition-colors"
                >
                  {v}
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>
      <div className="pt-1.5 border-t border-border/40 space-y-1">
        <p className="text-[9px] font-semibold text-text-muted uppercase tracking-wider mb-1">
          Examples
        </p>
        <div className="space-y-0.5 text-[10px] text-text-muted font-mono">
          <p>
            <span className="text-text-secondary">{'{filename}_Part_{num3}'}</span>{' '}
            <span>→ Course_Part_001.mp4</span>
          </p>
          <p>
            <span className="text-text-secondary">{'{chapter}_{num2}'}</span>{' '}
            <span>→ Introduction_01.mp4</span>
          </p>
          <p>
            <span className="text-text-secondary">{'{filename}_{start}_{end}'}</span>{' '}
            <span>→ Course_00-00-00_00-30-00.mp4</span>
          </p>
        </div>
      </div>
    </div>
  );
}