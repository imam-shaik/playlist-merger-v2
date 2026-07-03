import { useEffect, useRef } from 'react';
import { useSectionStore } from '@/store/sectionStore';
import { useAppStore } from '@/store/appStore';
import { sectionCommands, tauriEvents } from '@/tauri/commands';
import type { SectionEvent } from '@/types';

export function useSectionEvents() {
  const setCurrentSectionIndex = useSectionStore((s) => s.setCurrentSectionIndex);
  const setTotalSections = useSectionStore((s) => s.setTotalSections);
  const setSectionResults = useSectionStore((s) => s.setSectionResults);
  const setIsExecuting = useSectionStore((s) => s.setIsExecuting);
  const showToast = useAppStore((s) => s.showToast);

  const unlistenRef = useRef<Array<() => void>>([]);

  useEffect(() => {
    let cancelled = false;
    const fns: Array<() => void> = [];

    const setup = async () => {
      try {
        const u1 = await tauriEvents.onMergeProgress((_event) => {
          if (cancelled) return;
        });
        fns.push(u1);

        const u2 = await sectionCommands.onSectionStart((event: SectionEvent) => {
          if (cancelled) return;
          console.log(`[EVENT_RECEIVED] event=section-start jobId=${event.jobId} section=${event.currentSection}/${event.totalSections}`);
          setCurrentSectionIndex(event.currentSection);
          setTotalSections(event.totalSections);
          setIsExecuting(true);
        });
        fns.push(u2);

        const u3 = await sectionCommands.onSectionComplete((event: SectionEvent) => {
          if (cancelled) return;
          console.log(`[EVENT_RECEIVED] event=section-complete jobId=${event.jobId} section=${event.currentSection}`);
          setCurrentSectionIndex(event.currentSection);
        });
        fns.push(u3);

        const u4 = await sectionCommands.onSectionError((event: SectionEvent) => {
          if (cancelled) return;
          console.log(`[EVENT_RECEIVED] event=section-error jobId=${event.jobId} section=${event.currentSection} error=${event.error}`);
          setIsExecuting(false);
          showToast({
            type: 'error',
            title: 'Section failed',
            description: event.error || `Section ${event.currentSection + 1} failed`,
            durationMs: 8000,
          });
        });
        fns.push(u4);

        const u5 = await sectionCommands.onSectionMergeComplete(async (event) => {
          if (cancelled) return;
          console.log(`[EVENT_RECEIVED] event=section-merge-complete jobId=${event.jobId} success=${event.success}`);
          setIsExecuting(false);
          setSectionResults(event.results || []);
          if (event.success) {
            showToast({
              type: 'success',
              title: 'Section merge complete',
              description: `${event.successCount}/${event.totalCount} sections merged successfully`,
              durationMs: 6000,
            });
          }
        });
        fns.push(u5);

        const u6 = await sectionCommands.onSectionMergeError(async (event) => {
          if (cancelled) return;
          console.log(`[EVENT_RECEIVED] event=section-merge-error jobId=${event.jobId} error=${event.error}`);
          setIsExecuting(false);
          showToast({
            type: 'error',
            title: 'Section merge failed',
            description: event.error || 'Unknown error occurred',
            durationMs: 8000,
          });
        });
        fns.push(u6);

        if (!cancelled) {
          unlistenRef.current = fns;
        } else {
          fns.forEach((fn) => fn());
        }
      } catch (err) {
        console.error('Failed to set up section event listeners:', err);
      }
    };

    setup();

    return () => {
      cancelled = true;
      unlistenRef.current.forEach((fn) => fn());
      unlistenRef.current = [];
    };
  }, []);
}
