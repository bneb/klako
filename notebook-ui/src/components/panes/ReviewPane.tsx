import React, { useState, useEffect, useRef } from 'react';
import { useStore } from '../../store/useStore';
import ReactMarkdown from 'react-markdown';
import { motion, AnimatePresence } from 'framer-motion';

export const ReviewPane: React.FC = () => {
  const { reviewFilePath, reviewContent, reviewVisible } = useStore();
  const [popupStyle, setPopupStyle] = useState<{ top: number, left: number, visible: boolean }>({ top: 0, left: 0, visible: false });
  const [selectedText, setSelectedText] = useState('');
  const paneRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleSelection = () => {
      if (!paneRef.current) return;
      const selection = window.getSelection();
      const text = selection?.toString().trim();

      if (text && text.length > 0 && paneRef.current.contains(selection?.anchorNode || null)) {
        const range = selection?.getRangeAt(0);
        const rect = range?.getBoundingClientRect();
        if (rect) {
          setPopupStyle({
            top: rect.top - 40,
            left: rect.left + rect.width / 2,
            visible: true
          });
          setSelectedText(text);
        }
      } else {
        setPopupStyle(prev => ({ ...prev, visible: false }));
      }
    };

    document.addEventListener('selectionchange', handleSelection);
    return () => document.removeEventListener('selectionchange', handleSelection);
  }, []);

  const handleDiscuss = () => {
    // We would ideally call sendPrompt from useWebSocket here, but we can just use the DOM event or dispatch an action
    // For now, let's just log it or simulate it since we haven't wired the prompt sending back up to the top level App yet.
    // Real implementation would pass sendPrompt down or have a global event.
    const promptText = `I have a question about this section of the design document:\n\n> ${selectedText}\n\nCan you review this using SymbolWorld and give me your thoughts?`;
    console.log("Dispatching:", promptText);
    
    // In a real app we'd use the websocket provider here
    const event = new CustomEvent('klako-send-prompt', { detail: { text: promptText } });
    window.dispatchEvent(event);

    setPopupStyle(prev => ({ ...prev, visible: false }));
    window.getSelection()?.removeAllRanges();
  };

  if (!reviewVisible) return null;

  return (
    <div className="flex flex-col flex-1 bg-white border border-gray-200 rounded-xl shadow-sm overflow-hidden relative">
      <div className="bg-gray-100/50 border-b border-gray-200 px-4 py-3 font-semibold text-gray-700 text-sm flex justify-between items-center">
        <span>Review: {reviewFilePath || 'Unknown File'}</span>
        <button onClick={() => useStore.setState({ reviewVisible: false })} className="text-gray-400 hover:text-gray-600">✕</button>
      </div>
      <div ref={paneRef} className="flex-1 p-6 overflow-y-auto prose prose-sm max-w-none text-gray-800">
        <ReactMarkdown>{reviewContent || '*Empty document*'}</ReactMarkdown>
      </div>

      <AnimatePresence>
        {popupStyle.visible && (
          <motion.div
            initial={{ opacity: 0, scale: 0.9, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.9, y: 5 }}
            style={{
              position: 'fixed',
              top: popupStyle.top,
              left: popupStyle.left,
              transform: 'translateX(-50%)',
              zIndex: 50
            }}
            className="bg-indigo-600 text-white px-3 py-1.5 rounded shadow-lg text-xs font-medium cursor-pointer hover:bg-indigo-700 whitespace-nowrap"
            onClick={handleDiscuss}
          >
            Discuss with Klako
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
};
