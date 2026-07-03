// ────────────────────────────────────────────────
// OutputSettings — Output filename input with
// mode selector integration and filename validation.
// ────────────────────────────────────────────────

import React, { useState, useCallback } from 'react';
import { Input } from '@/components/ui/Input';
import { useMergeStore } from '@/store/mergeStore';
import { validateFilename } from '@/utils';

export function OutputSettings() {
  const outputFilename = useMergeStore((s) => s.outputFilename);
  const setOutputFilename = useMergeStore((s) => s.setOutputFilename);
  const [touched, setTouched] = useState(false);

  const error = touched ? validateFilename(outputFilename) : null;

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    setOutputFilename(e.target.value);
  }, [setOutputFilename]);

  const handleBlur = useCallback(() => {
    setTouched(true);
  }, []);

  return (
    <Input
      label="Output Filename"
      value={outputFilename}
      onChange={handleChange}
      onBlur={handleBlur}
      placeholder="merged_output"
      error={error ?? undefined}
      autoComplete="off"
      spellCheck={false}
    />
  );
}