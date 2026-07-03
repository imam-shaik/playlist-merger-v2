import { useCallback, useRef } from 'react';
import { usePlaylistStore } from '@/store/playlistStore';
import { tauriCommands } from '@/tauri/commands';
import { PROBE, THUMBNAIL } from '@/constants';

// ── Performance instrumentation counters ────────────────────────────────────
let _probeStartTime = 0;
let _probeCompletedCount = 0;
let _probeTotalCount = 0;
let _probeFirstResultMs = 0;
let _probeFirstFile = '';

/** Reset probe counters for a new batch */
function resetProbeCounters() {
  _probeStartTime = 0;
  _probeCompletedCount = 0;
  _probeTotalCount = 0;
  _probeFirstResultMs = 0;
  _probeFirstFile = '';
}

/** Log probe batch summary — called when all probes complete */
function logProbeSummary() {
  if (_probeTotalCount === 0) return;
  const elapsed = performance.now() - _probeStartTime;
  const avg = _probeCompletedCount > 0 ? (elapsed / _probeCompletedCount).toFixed(1) : '?';
  console.log(
    `[Perf:Probe] Batch complete: ${_probeCompletedCount}/${_probeTotalCount} files | ` +
    `Total: ${(elapsed / 1000).toFixed(1)}s | ` +
    `Avg: ${avg}ms/file | ` +
    `Concurrency: ${PROBE.MAX_CONCURRENT} | ` +
    `First result: ${_probeFirstResultMs.toFixed(0)}ms ("${_probeFirstFile}")`
  );
  resetProbeCounters();
}

/**
 * Batch probe results into a single store update every ~60ms.
 * This prevents a re-render storm when many files complete probing rapidly.
 */
import type { MediaInfo } from '@/types';

function createProbeBatcher(store: {
  batchSetMediaInfo: (updates: Map<string, MediaInfo>) => void;
}) {
  const pending = new Map<string, MediaInfo>();
  let timer: ReturnType<typeof setTimeout> | null = null;

  function flush() {
    if (pending.size === 0) return;
    const batch = new Map(pending);
    pending.clear();
    timer = null;
    store.batchSetMediaInfo(batch);
  }

  return {
    add: (id: string, info: MediaInfo) => {
      pending.set(id, info);
      if (pending.size >= 5) {
        // Count threshold reached — flush immediately
        if (timer) clearTimeout(timer);
        timer = null;
        flush();
      } else if (!timer) {
        // First item — schedule flush after 100ms
        timer = setTimeout(flush, 100);
      }
    },
    flushNow: flush,
    pendingCount: () => pending.size,
  };
}

/** Probe entries that don't have media info yet, with concurrency limit */
export function useProbe() {
  const store = usePlaylistStore();
  const inFlightRef = useRef(new Set<string>());
  const queueRef = useRef<string[]>([]);
  const activeCountRef = useRef(0);
  const batcherRef = useRef<ReturnType<typeof createProbeBatcher> | null>(null);

  // Lazily initialize batcher
  if (!batcherRef.current) {
    batcherRef.current = createProbeBatcher(store);
  }

  const processNext = useCallback(async () => {
    while (
      activeCountRef.current < PROBE.MAX_CONCURRENT &&
      queueRef.current.length > 0
    ) {
      const id = queueRef.current.shift()!;
      const entry = usePlaylistStore.getState().entries.find(e => e.id === id);

      // Skip if already probed or removed
      if (!entry || entry.mediaInfo || inFlightRef.current.has(id)) continue;

      inFlightRef.current.add(id);
      activeCountRef.current++;
      store.setProbing(id, true);

      tauriCommands
        .probeVideo(entry.path)
        .then((info) => {
          if (_probeCompletedCount === 0) {
            _probeFirstResultMs = performance.now() - _probeStartTime;
            _probeFirstFile = entry.path;
          }
          _probeCompletedCount++;

          // Batch the result instead of updating the store immediately
          batcherRef.current?.add(id, info);
          store.setProbing(id, false);
        })
        .catch((err) => {
          console.warn(`Probe failed for "${entry.name}":`, err);
          store.setProbing(id, false);
          _probeCompletedCount++;
        })
        .finally(() => {
          inFlightRef.current.delete(id);
          activeCountRef.current--;
          processNext();

          // [Perf] Log summary when all probes are done — flush remaining batched results
          if (_probeTotalCount > 0 && _probeCompletedCount >= _probeTotalCount && activeCountRef.current === 0) {
            batcherRef.current?.flushNow();
            logProbeSummary();
          }
        });
    }
  }, [store]);

  /** Queue a list of entry IDs for probing */
  const probeEntries = useCallback(
    (ids: string[]) => {
      const entries = usePlaylistStore.getState().entries;
      const toProbe = ids.filter((id) => {
        const e = entries.find(en => en.id === id);
        return e && !e.mediaInfo && !e.isProbing && !inFlightRef.current.has(id);
      });

      // [Perf] Initialize counters when queue is empty (new batch)
      if (queueRef.current.length === 0) {
        resetProbeCounters();
        _probeStartTime = performance.now();
        _probeTotalCount = toProbe.length;
  
      } else {
        _probeTotalCount += toProbe.length;
      }

      queueRef.current.push(...toProbe);
      processNext();
    },
    [processNext]
  );

  /** Probe all unprobed entries */
  const probeAll = useCallback(() => {
    const ids = usePlaylistStore
      .getState()
      .entries.filter(e => !e.mediaInfo && !e.isProbing)
      .map(e => e.id);
    probeEntries(ids);
  }, [probeEntries]);

  return { probeEntries, probeAll };
}

function createThumbnailBatcher(store: {
  batchSetThumbnailPath: (updates: Map<string, string | null>) => void;
}) {
  const pending = new Map<string, string | null>();
  let timer: ReturnType<typeof setTimeout> | null = null;

  function flush() {
    if (pending.size === 0) return;
    const batch = new Map(pending);
    pending.clear();
    timer = null;
    store.batchSetThumbnailPath(batch);
  }

  return {
    add: (id: string, path: string | null) => {
      pending.set(id, path);
      if (pending.size >= 10) {
        if (timer) clearTimeout(timer);
        timer = null;
        flush();
      } else if (!timer) {
        timer = setTimeout(flush, 150);
      }
    },
    flushNow: flush,
  };
}

/** Generate thumbnails for entries that don't have one yet */
export function useThumbnails() {
  const store = usePlaylistStore();
  const inFlightRef = useRef(new Set<string>());
  const activeCountRef = useRef(0);
  const batcherRef = useRef<ReturnType<typeof createThumbnailBatcher> | null>(null);

  if (!batcherRef.current) {
    batcherRef.current = createThumbnailBatcher(store);
  }

// ── Thumbnail performance counters ────────────────────────────────────────────
let _thumbStartTime = 0;
let _thumbCompletedCount = 0;
let _thumbTotalCount = 0;
let _thumbFirstResultMs = 0;

function resetThumbCounters() {
  _thumbStartTime = 0;
  _thumbCompletedCount = 0;
  _thumbTotalCount = 0;
  _thumbFirstResultMs = 0;
}

function logThumbSummary() {
  if (_thumbTotalCount === 0) return;
  const elapsed = performance.now() - _thumbStartTime;
  console.log(
    `[Perf:Thumbnail] Batch complete: ${_thumbCompletedCount}/${_thumbTotalCount} files | ` +
    `Total: ${(elapsed / 1000).toFixed(1)}s | ` +
    `Concurrency: ${THUMBNAIL.MAX_CONCURRENT} | ` +
    `First result: ${_thumbFirstResultMs.toFixed(0)}ms`
  );
  resetThumbCounters();
}

  const processNext = useCallback(async (queue: string[]) => {
    while (
      activeCountRef.current < THUMBNAIL.MAX_CONCURRENT &&
      queue.length > 0
    ) {
      const id = queue.shift()!;
      const entry = usePlaylistStore.getState().entries.find(e => e.id === id);

      if (!entry || entry.thumbnailPath || inFlightRef.current.has(id)) continue;

      inFlightRef.current.add(id);
      activeCountRef.current++;
      store.setLoadingThumbnail(id, true);      tauriCommands
        .generateThumbnail(
          entry.path,
          THUMBNAIL.DEFAULT_SEEK_SECONDS,
          { width: THUMBNAIL.WIDTH, height: THUMBNAIL.HEIGHT }
        )
        .then((thumbPath) => {
          if (_thumbCompletedCount === 0) {
            // eslint-disable-next-line react-hooks/exhaustive-deps
            _thumbFirstResultMs = performance.now() - _thumbStartTime;
          }
          _thumbCompletedCount++;
          batcherRef.current?.add(id, thumbPath);
          store.setLoadingThumbnail(id, false);
        })
        .catch((err) => {
          _thumbCompletedCount++;
          console.warn(`Thumbnail failed for "${entry.name}":`, err);
          batcherRef.current?.add(id, null);
          store.setLoadingThumbnail(id, false);
        })
        .finally(() => {
          inFlightRef.current.delete(id);
          activeCountRef.current--;
          processNext(queue);

          // [Perf] Log summary when all thumbnails are done
          if (_thumbTotalCount > 0 && _thumbCompletedCount >= _thumbTotalCount && activeCountRef.current === 0) {
            batcherRef.current?.flushNow();
            logThumbSummary();
          }
        });
    }
  }, [store]);

  const generateThumbnails = useCallback(
    (ids: string[]) => {
      const entries = usePlaylistStore.getState().entries;
      const queue = ids.filter((id) => {
        const e = entries.find(en => en.id === id);
        return e && !e.thumbnailPath && !e.isLoadingThumbnail && !inFlightRef.current.has(id);
      });
      // [Perf] Initialize thumbnail counters for new batch
      resetThumbCounters();
      // eslint-disable-next-line react-hooks/exhaustive-deps
      _thumbStartTime = performance.now();
      // eslint-disable-next-line react-hooks/exhaustive-deps
      _thumbTotalCount = queue.length;

      processNext([...queue]);
    },
    [processNext]
  );

  return { generateThumbnails };
}
