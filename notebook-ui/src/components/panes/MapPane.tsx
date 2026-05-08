import React, { useMemo } from 'react';
import { useStore } from '../../store/useStore';
import { ReactFlow, Controls, Background, MiniMap } from '@xyflow/react';
import type { Node, Edge } from '@xyflow/react';
import '@xyflow/react/dist/style.css';

export const MapPane: React.FC = () => {
  const { mapData, mapVisible } = useStore();

  const { nodes, edges } = useMemo(() => {
    if (!mapData) return { nodes: [], edges: [] };

    const generatedNodes: Node[] = [];
    const generatedEdges: Edge[] = [];
    
    // Very basic grid layout for now
    const files = Object.keys(mapData);
    const cols = Math.ceil(Math.sqrt(files.length));
    
    files.forEach((filePath, idx) => {
      const fileData = mapData[filePath];
      
      const row = Math.floor(idx / cols);
      const col = idx % cols;
      
      const px = 100 + col * 350;
      const py = 100 + row * 250;
      
      generatedNodes.push({
        id: filePath,
        position: { x: px, y: py },
        data: { 
          label: (
            <div className="flex flex-col gap-2 p-2 w-64 max-w-xs break-all">
              <div className="font-bold text-sm text-indigo-700 border-b pb-1 break-words">{filePath}</div>
              <div className="text-xs text-gray-500">{fileData.line_count || 0} lines</div>
              {fileData.symbols && fileData.symbols.length > 0 && (
                <div className="flex flex-col gap-1 mt-2 text-xs bg-gray-50 p-2 rounded">
                  {fileData.symbols.slice(0, 5).map((s: any, i: number) => (
                    <div key={i} className="flex justify-between">
                      <span className="font-mono text-gray-700 truncate">{s.name}</span>
                      <span className="text-gray-400">L{s.line}</span>
                    </div>
                  ))}
                  {fileData.symbols.length > 5 && (
                    <div className="text-gray-400 italic">+{fileData.symbols.length - 5} more</div>
                  )}
                </div>
              )}
            </div>
          ) 
        },
        className: 'bg-white border-2 border-indigo-200 rounded-xl shadow-md cursor-grab active:cursor-grabbing',
      });
    });

    return { nodes: generatedNodes, edges: generatedEdges };
  }, [mapData]);

  if (!mapVisible) return null;

  return (
    <div className="flex flex-col flex-1 bg-white border border-gray-200 rounded-xl shadow-sm overflow-hidden relative">
      <div className="bg-gray-100/50 border-b border-gray-200 px-4 py-3 font-semibold text-gray-700 text-sm flex justify-between items-center z-10">
        <span>Architecture Map</span>
        <button onClick={() => useStore.setState({ mapVisible: false })} className="text-gray-400 hover:text-gray-600">✕</button>
      </div>
      <div className="flex-1 w-full h-full relative">
        <ReactFlow nodes={nodes} edges={edges} fitView>
          <Background color="#ccc" gap={16} />
          <Controls />
          <MiniMap zoomable pannable />
        </ReactFlow>
      </div>
    </div>
  );
};
