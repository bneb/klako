import { useEffect } from 'react';
import { SprintBoard } from './components/panes/SprintBoard';
import { ReviewPane } from './components/panes/ReviewPane';
import { MapPane } from './components/panes/MapPane';
import { useWebSocket } from './providers/useWebSocket';
import { useStore } from './store/useStore';

function App() {
  const { sendPrompt } = useWebSocket();
  const { status } = useStore();

  useEffect(() => {
    const handleSendPrompt = (e: Event) => {
      const customEvent = e as CustomEvent<{text: string}>;
      if (customEvent.detail && customEvent.detail.text) {
        sendPrompt(customEvent.detail.text);
      }
    };
    window.addEventListener('klako-send-prompt', handleSendPrompt);
    return () => window.removeEventListener('klako-send-prompt', handleSendPrompt);
  }, [sendPrompt]);

  return (
    <div className="flex h-screen w-screen bg-gray-50 text-gray-900 font-sans">
      {/* Sidebar Navigation */}
      <div className="w-16 flex flex-col items-center py-4 border-r border-gray-200 bg-white z-10">
        <div className="w-8 h-8 rounded-full bg-indigo-600 flex items-center justify-center text-white font-bold mb-8 shadow-md">
          K
        </div>
        {/* Placeholder for icons */}
        <div className="flex flex-col gap-6 text-gray-400">
          <div className="cursor-pointer hover:text-indigo-600 transition-colors">⌘</div>
          <div className="cursor-pointer hover:text-indigo-600 transition-colors">▤</div>
        </div>
      </div>

      {/* Main Content Area */}
      <div className="flex-1 flex flex-col h-full overflow-hidden">
        {/* Header */}
        <header className="h-14 border-b border-gray-200 bg-white flex items-center px-6 justify-between shadow-sm z-10">
          <h1 className="font-semibold text-lg tracking-tight">Klako Command Center</h1>
          <div className="flex items-center gap-4">
            <span className={`text-xs font-mono px-2 py-1 rounded-full font-medium ${status === 'idle' ? 'bg-emerald-100 text-emerald-700' : 'bg-amber-100 text-amber-700'}`}>
              {status === 'idle' ? 'Connected' : 'Working...'}
            </span>
          </div>
        </header>

        {/* Panes Area */}
        <main className="flex-1 flex overflow-hidden p-4 gap-4">
          <SprintBoard />
          <MapPane />
          <ReviewPane />
        </main>
      </div>
    </div>
  );
}

export default App;
