import { useCallback, useRef } from 'react';
import { usePlaylistStore } from '@/store/playlistStore';
import { tauriCommands } from '@/tauri/commands';

/**
 * Health check state for a single entry
 */
export interface HealthCheckState {
  isChecking: boolean;
  lastChecked: number | null;
  error: string | null;
}

/**
 * Hook to manage file health checks.
 *
 * Usage:
 *   const { checkHealth, checkHealthForEntry } = useHealthCheck();
 *   // Then call checkHealth() to trigger health checks for all entries
 */
export function useHealthCheck() {
  // Ref to track current health check generation to prevent stale updates
  const healthGenerationRef = useRef(0);

  const checkHealth = useCallback(async (): Promise<void> => {
    const entries = usePlaylistStore.getState().entries;
    if (entries.length === 0) return;

    const currentGeneration = ++healthGenerationRef.current;

    try {
      const paths = entries.map((e) => e.path);
      const report = await tauriCommands.checkFileHealth(paths);

      // Guard against stale results
      if (currentGeneration !== healthGenerationRef.current) return;

      const ps = usePlaylistStore.getState();

      // Match results to entries by path and update health
      for (const health of report.perFile) {
        const entry = health.path
          ? entries.find(
              (e) =>
                e.path === health.path ||
                e.path.replace(/\\\\/g, '/') === health.path!.replace(/\\\\/g, '/')
            )
          : undefined;

        if (entry) {
          ps.setHealth(entry.id, {
            status: health.status,
            message: health.message,
            technicalDetails: health.technicalDetails,
            confidence: health.confidence,
            canMergeLossless: health.canMergeLossless,
            canMergeCustom: health.canMergeCustom,
            autoRepair: health.autoRepair,
          });
        }
      }
    } catch (err) {
      console.error('[HealthCheck] Health check failed:', err);
      throw err;
    }
  }, []);

  /**
   * Check health for a single entry by ID
   */
  const checkHealthForEntry = useCallback(
    async (entryId: string): Promise<void> => {
      const entries = usePlaylistStore.getState().entries;
      const entry = entries.find((e) => e.id === entryId);
      if (!entry) return;

      const currentGeneration = ++healthGenerationRef.current;

      try {
        const report = await tauriCommands.checkFileHealth([entry.path]);

        if (currentGeneration !== healthGenerationRef.current) return;

        if (report.perFile.length > 0) {
          const health = report.perFile[0];
          usePlaylistStore.getState().setHealth(entry.id, {
            status: health.status,
            message: health.message,
            technicalDetails: health.technicalDetails,
            confidence: health.confidence,
            canMergeLossless: health.canMergeLossless,
            canMergeCustom: health.canMergeCustom,
            autoRepair: health.autoRepair,
          });
        }
      } catch (err) {
        console.error(`Health check failed for ${entry.path}:`, err);
        throw err;
      }
    },
    []
  );

  return {
    checkHealth,
    checkHealthForEntry,
  };
}
