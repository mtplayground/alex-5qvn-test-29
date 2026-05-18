import { useEffect, useRef, useState } from 'react'
import cytoscape, { type LayoutOptions } from 'cytoscape'
import coseBilkent from 'cytoscape-cose-bilkent'

import { cn } from '@/lib/utils'
import type { GraphEdge, GraphNode } from '@/lib/api-types'

cytoscape.use(coseBilkent)

declare global {
  interface Window {
    __graphCanvasDebug?: {
      dragNodeBy: (nodeId: string, deltaX: number, deltaY: number) => boolean
      getNodePositions: () => Array<{
        id: string
        label: string
        renderedX: number
        renderedY: number
      }>
    }
  }
}

const palette = [
  '#0f766e',
  '#b45309',
  '#1d4ed8',
  '#be123c',
  '#6d28d9',
  '#15803d',
  '#c2410c',
  '#334155',
] as const

export type GraphCanvasProps = {
  nodes: GraphNode[]
  edges: GraphEdge[]
  selectedNodeId?: string | null
  onNodeSelect?: (nodeId: string) => void
  onNodeDoubleClick?: (nodeId: string) => void
  onCanvasClear?: () => void
  emptyMessage?: string
  className?: string
}

export function GraphCanvas({
  nodes,
  edges,
  selectedNodeId,
  onNodeSelect,
  onNodeDoubleClick,
  onCanvasClear,
  emptyMessage,
  className,
}: GraphCanvasProps) {
  const [nodePositions, setNodePositions] = useState<Array<{
    id: string
    label: string
    renderedX: number
    renderedY: number
  }>>([])
  const containerRef = useRef<HTMLDivElement | null>(null)
  const cyRef = useRef<cytoscape.Core | null>(null)
  const onNodeSelectRef = useRef(onNodeSelect)
  const onNodeDoubleClickRef = useRef(onNodeDoubleClick)
  const onCanvasClearRef = useRef(onCanvasClear)
  const lastTapRef = useRef<{ nodeId: string; timestamp: number } | null>(null)

  useEffect(() => {
    onNodeSelectRef.current = onNodeSelect
    onNodeDoubleClickRef.current = onNodeDoubleClick
    onCanvasClearRef.current = onCanvasClear
  }, [onCanvasClear, onNodeDoubleClick, onNodeSelect])

  useEffect(() => {
    if (!containerRef.current || nodes.length === 0) {
      return
    }

    const elements = [
      ...nodes.map((node) => {
        const labelKey = node.labels.join('|') || 'Node'
        return {
          data: {
            id: node.id,
            label: node.properties.name ?? node.properties.slug ?? node.id,
            meta: node.labels.join(' · '),
            tone: colorForSeed(labelKey),
          },
        }
      }),
      ...edges.map((edge) => ({
        data: {
          id: edge.id,
          source: edge.start_id,
          target: edge.end_id,
          label: edge.type,
          tone: colorForSeed(edge.type),
        },
      })),
    ]

    const cy = cytoscape({
      container: containerRef.current,
      elements,
      minZoom: 0.3,
      maxZoom: 2.4,
      wheelSensitivity: 0.18,
      textureOnViewport: true,
      style: [
        {
          selector: 'node',
          style: {
            'background-color': 'data(tone)',
            'border-width': 2,
            'border-color': '#f8fafc',
            'label': 'data(label)',
            'text-wrap': 'wrap',
            'text-max-width': '140px',
            'font-size': '11px',
            'font-weight': 700,
            'color': '#102a43',
            'text-valign': 'center',
            'text-halign': 'center',
            'width': '52px',
            'height': '52px',
            'overlay-padding': 6,
            'overlay-opacity': 0,
          },
        },
        {
          selector: 'edge',
          style: {
            'curve-style': 'bezier',
            'width': 3,
            'line-color': 'data(tone)',
            'target-arrow-color': 'data(tone)',
            'target-arrow-shape': 'triangle',
            'arrow-scale': 1.1,
            'label': 'data(label)',
            'font-size': '9px',
            'font-weight': 700,
            'text-background-color': '#fffdf8',
            'text-background-opacity': 0.92,
            'text-background-padding': '3px',
            'text-rotation': 'autorotate',
            'color': '#334155',
          },
        },
        {
          selector: 'node:selected',
          style: {
            'border-width': 4,
            'border-color': '#102a43',
          },
        },
      ],
      layout: {
        name: 'cose-bilkent',
        animate: false,
        fit: true,
        padding: 36,
        nodeRepulsion: 5200,
        idealEdgeLength: 140,
        edgeElasticity: 0.35,
        gravity: 0.2,
        nestingFactor: 0.8,
      } as unknown as LayoutOptions,
    })

    cyRef.current = cy
    syncNodePositions(cy, setNodePositions)
    const initialPositionTimer = window.setTimeout(() => {
      syncNodePositions(cy, setNodePositions)
    }, 0)
    window.__graphCanvasDebug = {
      dragNodeBy: (nodeId, deltaX, deltaY) => {
        const node = cy.$id(nodeId)

        if (node.empty()) {
          return false
        }

        const currentPosition = node.position()
        node.position({
          x: currentPosition.x + deltaX,
          y: currentPosition.y + deltaY,
        })
        syncNodePositions(cy, setNodePositions)
        return true
      },
      getNodePositions: () =>
        cy.nodes().map((node) => {
          const renderedPosition = node.renderedPosition()

          return {
            id: node.id(),
            label: String(node.data('label') ?? ''),
            renderedX: Number(renderedPosition.x.toFixed(2)),
            renderedY: Number(renderedPosition.y.toFixed(2)),
          }
        }),
    }

    cy.on('tap', 'node', (event) => {
      const nodeId = event.target.id()
      const now = Date.now()
      const lastTap = lastTapRef.current

      onNodeSelectRef.current?.(nodeId)

      if (lastTap && lastTap.nodeId === nodeId && now - lastTap.timestamp < 320) {
        lastTapRef.current = null
        onNodeDoubleClickRef.current?.(nodeId)
        return
      }

      lastTapRef.current = {
        nodeId,
        timestamp: now,
      }
    })

    cy.on('tap', (event) => {
      if (event.target === cy) {
        lastTapRef.current = null
        cy.elements().unselect()
        onCanvasClearRef.current?.()
      }
    })

    cy.on('dragfree', 'node', () => {
      syncNodePositions(cy, setNodePositions)
    })

    if (selectedNodeId) {
      const selectedNode = cy.$id(selectedNodeId)

      if (selectedNode.nonempty()) {
        selectedNode.select()
      }
    }

    return () => {
      window.clearTimeout(initialPositionTimer)
      delete window.__graphCanvasDebug
      cyRef.current = null
      setNodePositions([])
      cy.destroy()
    }
  }, [edges, nodes])

  useEffect(() => {
    const cy = cyRef.current

    if (!cy) {
      return
    }

    cy.elements().unselect()

    if (!selectedNodeId) {
      return
    }

    const selectedNode = cy.$id(selectedNodeId)

    if (selectedNode.nonempty()) {
      selectedNode.select()
    }
  }, [selectedNodeId])

  return (
    <div
      data-testid="graph-canvas"
      className={cn(
        'relative overflow-hidden rounded-[1.75rem] border border-border/70 bg-[linear-gradient(160deg,_rgba(255,255,255,0.94),_rgba(235,248,245,0.96))] shadow-panel',
        className,
      )}
    >
      <div className="flex items-center justify-between border-b border-border/60 px-5 py-3 text-xs font-medium uppercase tracking-[0.24em] text-muted-foreground">
        <span>Graph canvas</span>
        <span>Drag, pan, zoom</span>
      </div>
      <div className="relative h-[28rem]">
        {nodes.length > 0 ? (
          <div ref={containerRef} className="h-full w-full" data-testid="graph-viewport" />
        ) : (
          <div className="flex h-full items-center justify-center px-8 text-center text-sm text-muted-foreground">
            {emptyMessage ?? 'Run a Cypher query or select a node expansion to populate the canvas.'}
          </div>
        )}
      </div>
      <pre className="sr-only" data-testid="graph-node-positions">
        {JSON.stringify(nodePositions)}
      </pre>
    </div>
  )
}

function colorForSeed(seed: string) {
  let hash = 0

  for (let index = 0; index < seed.length; index += 1) {
    hash = (hash * 31 + seed.charCodeAt(index)) >>> 0
  }

  return palette[hash % palette.length]
}

function syncNodePositions(
  cy: cytoscape.Core,
  setNodePositions: (
    value: Array<{
      id: string
      label: string
      renderedX: number
      renderedY: number
    }>,
  ) => void,
) {
  setNodePositions(
    cy.nodes().map((node) => {
      const renderedPosition = node.renderedPosition()

      return {
        id: node.id(),
        label: String(node.data('label') ?? ''),
        renderedX: Number(renderedPosition.x.toFixed(2)),
        renderedY: Number(renderedPosition.y.toFixed(2)),
      }
    }),
  )
}
