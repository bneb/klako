import { useEffect, useRef } from 'react';
import { useStore } from '../store/useStore';

export const useWebSocket = () => {
  const wsRef = useRef<WebSocket | null>(null);
  const {
    handleStatusUpdate,
    handleNarrativeDelta,
    handleSwarmLedgerUpdate,
    handleCanvasTelemetry,
  } = useStore();

  useEffect(() => {
    // Only connect if we aren't in a test environment
    if (import.meta.env.MODE === 'test') return;
    
    // Check if we are forcing mock mode
    const isMock = new URLSearchParams(window.location.search).has('mock');
    if (isMock) {
      console.log('[WebSocket] Running in Mock Mode. Connection skipped.');
      return;
    }

    const connect = () => {
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const wsUrl = `${protocol}//${window.location.host}/stream`;
      
      console.log(`[WebSocket] Connecting to ${wsUrl}...`);
      const ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        console.log('[WebSocket] Connected');
        handleCanvasTelemetry({ line: '[System] Connected to Rust Kernel via WebSocket.' });
      };

      ws.onmessage = (event) => {
        try {
          const payload = JSON.parse(event.data);
          
          switch (payload.type) {
            case 'StatusUpdate':
              handleStatusUpdate(payload);
              break;
            case 'NarrativeDelta':
              handleNarrativeDelta(payload);
              break;
            case 'SwarmLedgerUpdate':
              handleSwarmLedgerUpdate(payload);
              break;
            case 'CanvasTelemetry':
              handleCanvasTelemetry(payload);
              break;
            case 'OpenReviewPane':
              useStore.setState({ reviewFilePath: payload.file_path, reviewContent: payload.content, reviewVisible: true });
              break;
            default:
              // Handle other events like PlanDelta, VisualArtifact later
              break;
          }
        } catch (err) {
          console.error('[WebSocket] Parse Error:', err);
        }
      };

      ws.onclose = () => {
        console.log('[WebSocket] Disconnected. Reconnecting in 2s...');
        handleStatusUpdate({ role: 'idle', tier: 'Disconnected' });
        setTimeout(connect, 2000);
      };

      ws.onerror = (err) => {
        console.error('[WebSocket] Error:', err);
        ws.close();
      };

      wsRef.current = ws;
    };

    connect();

    return () => {
      if (wsRef.current) {
        wsRef.current.close();
      }
    };
  }, [handleStatusUpdate, handleNarrativeDelta, handleSwarmLedgerUpdate, handleCanvasTelemetry]);

  const sendPrompt = (text: string) => {
    if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ type: 'SubmitPrompt', text }));
    } else {
      console.error('[WebSocket] Cannot send prompt, connection not open.');
    }
  };

  return { sendPrompt };
};
