import { useState, useCallback, DragEvent } from 'react';

interface UseDropZoneOptions {
  /** Called with the raw file paths from a native OS drop event */
  onDropFiles?: (paths: string[]) => void | Promise<unknown>;
}

export function useDropZone({ onDropFiles }: UseDropZoneOptions = {}) {
  const [isDragOver, setIsDragOver] = useState(false);

  const onDragEnter = useCallback((e: DragEvent) => {
    e.preventDefault();
    setIsDragOver(true);
  }, []);

  const onDragLeave = useCallback((e: DragEvent) => {
    e.preventDefault();
    if (e.currentTarget === e.target || !e.currentTarget.contains(e.relatedTarget as Node)) {
      setIsDragOver(false);
    }
  }, []);

  const onDragOver = useCallback((e: DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  }, []);

  const onDrop = useCallback(
    (e: DragEvent) => {
      e.preventDefault();
      setIsDragOver(false);

      if (e.dataTransfer.files.length > 0) {
        const paths: string[] = [];
        for (let i = 0; i < e.dataTransfer.files.length; i++) {
          const file = e.dataTransfer.files[i];
          const fullPath = (file as File & { path?: string }).path;
          if (fullPath) paths.push(fullPath);
        }
        onDropFiles?.(paths);
      }
    },
    [onDropFiles]
  );

  return {
    isDragOver,
    dropHandlers: { onDragEnter, onDragLeave, onDragOver, onDrop },
  };
}
