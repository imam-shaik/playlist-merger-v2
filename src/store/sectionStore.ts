import { create } from 'zustand';
import type { 
  PartitionMethod, 
  SectionPlan, 
  FolderForMerge,
  SectionResult 
} from '@/types';

type PartitionMethodType = 'sectionCount' | 'maxDuration' | 'maxSize';

interface SectionState {
  // Configuration
  methodType: PartitionMethodType;
  sectionCount: number;
  maxDurationSecs: number;
  maxSizeBytes: number;
  nameTemplate: string;
  
  // Computed preview
  sectionPlans: SectionPlan[];
  isCalculating: boolean;
  
  // Folder exclusion
  folderExclusions: Record<string, boolean>;
  
  // Execution state
  isExecuting: boolean;
  currentSectionIndex: number;
  totalSections: number;
  sectionResults: SectionResult[];
  
  // Actions
  setMethodType: (type: PartitionMethodType) => void;
  setSectionCount: (count: number) => void;
  setMaxDurationSecs: (secs: number) => void;
  setMaxSizeBytes: (bytes: number) => void;
  setNameTemplate: (template: string) => void;
  setFolderExclusion: (folderPath: string, included: boolean) => void;
  setSectionPlans: (plans: SectionPlan[]) => void;
  setIsCalculating: (v: boolean) => void;
  setIsExecuting: (v: boolean) => void;
  setCurrentSectionIndex: (idx: number) => void;
  setTotalSections: (total: number) => void;
  setSectionResults: (results: SectionResult[]) => void;
  addSectionResult: (result: SectionResult) => void;
  
  // Helpers
  getPartitionMethod: () => PartitionMethod;
  getActiveFolders: (folders: FolderForMerge[]) => FolderForMerge[];
  reset: () => void;
}

const DEFAULT_NAME_TEMPLATE = 'Course_Part_{N}';

const initialState = {
  methodType: 'sectionCount' as PartitionMethodType,
  sectionCount: 4,
  maxDurationSecs: 4 * 3600,
  maxSizeBytes: 2 * 1024 * 1024 * 1024,
  nameTemplate: DEFAULT_NAME_TEMPLATE,
  sectionPlans: [],
  isCalculating: false,
  folderExclusions: {},
  isExecuting: false,
  currentSectionIndex: 0,
  totalSections: 0,
  sectionResults: [],
};

export const useSectionStore = create<SectionState>((set, get) => ({
  ...initialState,
  
  setMethodType: (type) => set({ methodType: type, sectionPlans: [] }),
  
  setSectionCount: (count) => set({ sectionCount: Math.max(2, Math.min(50, count)) }),
  
  setMaxDurationSecs: (secs) => set({ maxDurationSecs: secs }),
  
  setMaxSizeBytes: (bytes) => set({ maxSizeBytes: bytes }),
  
  setNameTemplate: (template) => set({ nameTemplate: template }),
  
  setFolderExclusion: (folderPath, included) => set((state) => ({
    folderExclusions: {
      ...state.folderExclusions,
      [folderPath]: included,
    },
  })),
  
  setSectionPlans: (plans) => set({ sectionPlans: plans }),
  
  setIsCalculating: (v) => set({ isCalculating: v }),
  
  setIsExecuting: (v) => set({ isExecuting: v }),
  
  setCurrentSectionIndex: (idx) => set({ currentSectionIndex: idx }),
  
  setTotalSections: (total) => set({ totalSections: total }),
  
  setSectionResults: (results) => set({ sectionResults: results }),
  
  addSectionResult: (result) => set((state) => ({
    sectionResults: [...state.sectionResults, result],
  })),
  
  getPartitionMethod: () => {
    const state = get();
    switch (state.methodType) {
      case 'sectionCount':
        return { sectionCount: state.sectionCount };
      case 'maxDuration':
        return { maxDurationSecs: state.maxDurationSecs };
      case 'maxSize':
        return { maxSizeBytes: state.maxSizeBytes };
    }
  },
  
  getActiveFolders: (folders) => {
    const state = get();
    return folders.filter((f) => state.folderExclusions[f.path] !== false);
  },
  
  reset: () => set(initialState),
}));