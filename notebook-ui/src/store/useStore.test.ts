import { describe, it, expect, beforeEach } from 'vitest';
import { useStore } from './useStore';

describe('useStore', () => {
  beforeEach(() => {
    // Reset state before each test
    useStore.setState({
      status: 'idle',
      tier: 'Idle',
      narrative: '',
      telemetry: [],
      swarmTasks: [],
      swarmAgents: [],
    });
  });

  it('should initialize with correct default state', () => {
    const state = useStore.getState();
    expect(state.status).toBe('idle');
    expect(state.narrative).toBe('');
    expect(state.telemetry).toEqual([]);
    expect(state.swarmTasks).toEqual([]);
    expect(state.swarmAgents).toEqual([]);
  });

  it('should handle StatusUpdate correctly', () => {
    useStore.getState().handleStatusUpdate({
      type: 'StatusUpdate',
      role: 'thinker',
      tier: 'Architect'
    });

    const state = useStore.getState();
    expect(state.status).toBe('thinker');
    expect(state.tier).toBe('Architect');
  });

  it('should handle NarrativeDelta correctly', () => {
    useStore.getState().handleNarrativeDelta({
      type: 'NarrativeDelta',
      text: 'Hello '
    });
    
    useStore.getState().handleNarrativeDelta({
      type: 'NarrativeDelta',
      text: 'World'
    });

    expect(useStore.getState().narrative).toBe('Hello World');
  });
});
