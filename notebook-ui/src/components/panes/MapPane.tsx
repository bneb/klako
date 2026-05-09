import React, { useMemo } from 'react';
import { useStore } from '../../store/useStore';
import { ReactFlow, Controls, Background, MiniMap, MarkerType } from '@xyflow/react';
import type { Node, Edge } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import dagre from 'dagre';

export const MapPane: React.FC = () => {
  const { mapData, mapVisible } = useStore();

  const { nodes, edges } = useMemo(() => {
    if (!mapData) return { nodes: [], edges: [] };

    const generatedNodes: Node[] = [];
    const generatedEdges: Edge[] = [];
    
    const files = Object.keys(mapData);
    
    // Optimization: Create a lookup map for faster dependency resolution
    const fileLookup = new Map<string, string>();
    files.forEach(f => {
        const parts = f.split('/');
        const baseName = parts[parts.length - 1].split('.')[0];
        fileLookup.set(baseName, f);
        fileLookup.set(f, f); // also map full path
    });
    
    files.forEach((filePath) => {
      const fileData = mapData[filePath];
      
      generatedNodes.push({
        id: filePath,
        position: { x: 0, y: 0 },
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

      // Generate Edges
      if (fileData.dependencies && Array.isArray(fileData.dependencies)) {
        fileData.dependencies.forEach((dep: string) => {
          const cleanDep = dep.replace('./', '').replace('../', '').split('.')[0];
          const targetNode = fileLookup.get(cleanDep) || files.find(f => f.includes(cleanDep) && f !== filePath);
          
          if (targetNode && targetNode !== filePath) {
            generatedEdges.push({
              id: `e-${filePath}-${targetNode}`,
              source: filePath,
              target: targetNode,
              animated: true,
              style: { stroke: '#818cf8', strokeWidth: 2 },
              markerEnd: {
                type: MarkerType.ArrowClosed,
                color: '#818cf8',
              },
            });
          }
        });
      }
    });

    // Apply Dagre Layout
    const dagreGraph = new dagre.graphlib.Graph();
    dagreGraph.setDefaultEdgeLabel(() => ({}));
    dagreGraph.setGraph({ rankdir: 'LR', ranksep: 150, nodesep: 100 }); // Left-to-Right layout is usually better for DAGs

    generatedNodes.forEach((node) => {
      dagreGraph.setNode(node.id, { width: 300, height: 150 });
    });

    generatedEdges.forEach((edge) => {
      dagreGraph.setEdge(edge.source, edge.target);
    });

    dagre.layout(dagreGraph);

    const layoutedNodes = generatedNodes.map((node) => {
      const nodeWithPosition = dagreGraph.node(node.id);
      node.position = {
        x: nodeWithPosition.x - 150,
        y: nodeWithPosition.y - 75,
      };
      return node;
    });

    return { nodes: layoutedNodes, edges: generatedEdges };
  }, [mapData]);

  if (!mapVisible) return null;

  return (
    <div className="flex flex-col flex-1 bg-white border border-gray-200 rounded-xl shadow-sm overflow-hidden relative">
      <div className="bg-gray-100/50 border-b border-gray-200 px-4 py-3 font-semibold text-gray-700 text-sm flex justify-between items-center z-10">
        <span>Architecture Map</span>
        <button onClick={() => useStore.setState({ mapVisible: false })} className="text-gray-400 hover:text-gray-600">✕</button>
      </div>
      <div className="flex-1 w-full h-full relative">
        <ReactFlow nodes={nodes} edges={edges} fitView minZoom={0.1}>
          <Background color="#ccc" gap={16} />
          <Controls />
          <MiniMap zoomable pannable />
        </ReactFlow>
      </div>
    </div>
  );
};
