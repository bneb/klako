import { create } from 'zustand';

export interface SwarmTask {
  description: string;
  status: 'Pending' | 'Running' | 'Verifying' | 'Completed' | 'Failed';
  verification_tool?: string;
}

export interface SwarmAgent {
  id: string;
  subagent_type: string;
  status: string;
}

interface KlakoState {
  status: string;
  tier: string;
  narrative: string;
  telemetry: string[];
  swarmTasks: SwarmTask[];
  swarmAgents: SwarmAgent[];
  
  reviewFilePath: string | null;
  reviewContent: string | null;
  reviewVisible: boolean;
  
  mapData: any | null;
  mapVisible: boolean;

  handleStatusUpdate: (payload: any) => void;
  handleNarrativeDelta: (payload: any) => void;
  handleSwarmLedgerUpdate: (payload: any) => void;
  handleCanvasTelemetry: (payload: any) => void;
  handleMapArtifact: (payload: any) => void;
}

export const useStore = create<KlakoState>((set) => ({
  status: 'idle',
  tier: 'Idle',
  narrative: '',
  telemetry: [],
  swarmTasks: [],
  swarmAgents: [],
  
  reviewFilePath: null,
  reviewContent: null,
  reviewVisible: false,
  
  mapData: null,
  mapVisible: false,

  handleStatusUpdate: (payload) => {
    set({ status: payload.role, tier: payload.tier });
  },

  handleNarrativeDelta: (payload) => {
    set((state) => ({ narrative: state.narrative + (payload.text || '') }));
  },

  handleSwarmLedgerUpdate: (payload) => {
    set({
      swarmTasks: payload.tasks || [],
      swarmAgents: payload.agents || []
    });
  },

  handleCanvasTelemetry: (payload) => {
    set((state) => {
      const newTelemetry = [...state.telemetry, payload.line];
      // Keep last 1000 lines max
      if (newTelemetry.length > 1000) {
        newTelemetry.shift();
      }
      return { telemetry: newTelemetry };
    });
  },
  
  handleMapArtifact: (payload) => {
      set({ mapData: payload.map_data, mapVisible: true });
  }
}));
