import { useEffect, useState } from 'react'
import { Activity, DatabaseZap, LoaderCircle, Network } from 'lucide-react'

import { CypherEditor } from '@/components/editor/cypher-editor'
import { GraphCanvas } from '@/components/graph/graph-canvas'
import { AppShell } from '@/components/layout/app-shell'
import { Button } from '@/components/ui/button'
import { ApiError } from '@/lib/api-client'
import {
  useCypherMutation,
  useNodeNeighborsMutation,
  useSchemaQuery,
} from '@/lib/api-hooks'
import type { GraphEdge, GraphNode, GraphResult, NodeNeighborsResult } from '@/lib/api-types'
import { isGraphEdge, isGraphNode } from '@/lib/api-types'

const highlights = [
  {
    title: 'Cypher workbench',
    description:
      'Compose graph queries with a focused editor surface and room for rich result views.',
    icon: Activity,
  },
  {
    title: 'PostgreSQL graph store',
    description:
      'Back the graph model with typed HTTP endpoints over a PostgreSQL-first Rust service.',
    icon: DatabaseZap,
  },
  {
    title: 'Visual exploration',
    description:
      'Reserve space for the graph canvas, schema navigation, and record inspection flows.',
    icon: Network,
  },
] as const

function App() {
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null)
  const [inspectedNodeId, setInspectedNodeId] = useState<string | null>(null)
  const [activeLabelScan, setActiveLabelScan] = useState<string | null>(null)
  const [queryText, setQueryText] = useState('MATCH (n) RETURN n LIMIT 25')
  const [canvasGraph, setCanvasGraph] = useState<GraphResult>({
    nodes: [],
    edges: [],
  })
  const schemaQuery = useSchemaQuery()
  const nodeExpandMutation = useNodeNeighborsMutation()
  const cypherMutation = useCypherMutation()
  const canvasNodes = canvasGraph.nodes
  const canvasEdges = canvasGraph.edges
  const inspectedNode =
    canvasNodes.find((node) => node.id === inspectedNodeId) ?? null

  useEffect(() => {
    if (!cypherMutation.data) {
      return
    }

    setCanvasGraph(cypherMutation.data.graph)
    setSelectedNodeId(null)
  }, [cypherMutation.data])

  useEffect(() => {
    if (!inspectedNodeId) {
      return
    }

    const hasSelectedNode = canvasNodes.some((node) => node.id === inspectedNodeId)

    if (!hasSelectedNode) {
      setInspectedNodeId(null)
    }
  }, [canvasNodes, inspectedNodeId])

  function handleExpandNode(nodeId: string) {
    setSelectedNodeId(nodeId)

    nodeExpandMutation.mutate(nodeId, {
      onSuccess: (result) => {
        setCanvasGraph((currentGraph) => mergeExpandedGraph(currentGraph, result))
      },
    })
  }

  function handleLabelScan(label: string) {
    setActiveLabelScan(label)
    setSelectedNodeId(null)
    setQueryText(`MATCH (n:${escapeCypherIdentifier(label)}) RETURN n LIMIT 50`)

    cypherMutation.mutate({
      query: `MATCH (n:${escapeCypherIdentifier(label)}) RETURN n LIMIT 50`,
    })
  }

  function handleRunQuery(query: string) {
    if (cypherMutation.isPending) {
      return
    }

    setActiveLabelScan(null)
    setSelectedNodeId(null)
    cypherMutation.mutate({
      query,
    })
  }

  return (
    <div>
      <AppShell
        badge="API client ready"
        title="Graph playground workspace"
        description="A typed frontend shell prepared for schema reads, node expansion, and Cypher execution over the Rust graph API."
        highlights={highlights}
      />

      <section className="container -mt-6 pb-12">
        <div className="grid gap-4 xl:grid-cols-[0.34fr_1fr_0.48fr]">
          <aside className="rounded-[1.75rem] border border-border/70 bg-card/92 p-6 shadow-panel backdrop-blur">
            <p className="text-xs font-semibold uppercase tracking-[0.24em] text-primary">
              Schema sidebar
            </p>
            <h3 className="mt-3 text-lg font-semibold">Catalog + quick scans</h3>
            <p className="mt-3 text-sm leading-6 text-muted-foreground">
              Labels and relationship types come from `GET /schema`. Click a label to run
              a capped node scan and replace the active canvas with those results.
            </p>

            <div className="mt-6 space-y-5">
              <section>
                <div className="flex items-center justify-between">
                  <p className="text-sm font-semibold text-foreground">Labels</p>
                  <span className="text-xs uppercase tracking-[0.18em] text-muted-foreground">
                    {schemaQuery.data?.labels.length ?? 0}
                  </span>
                </div>
                <div className="mt-3 space-y-2">
                  {schemaQuery.data?.labels.map(({ label, count }) => {
                    const isActive = activeLabelScan === label

                    return (
                      <button
                        key={label}
                        type="button"
                        className={[
                          'flex w-full items-center justify-between rounded-2xl border px-3 py-3 text-left text-sm transition-colors',
                          isActive
                            ? 'border-primary/40 bg-primary/10 text-foreground'
                            : 'border-border/70 bg-white/60 text-foreground hover:border-primary/25 hover:bg-primary/5',
                        ].join(' ')}
                        onClick={() => handleLabelScan(label)}
                      >
                        <span className="font-medium">{label}</span>
                        <span className="rounded-full bg-black/5 px-2.5 py-1 text-xs font-semibold text-muted-foreground">
                          {count}
                        </span>
                      </button>
                    )
                  })}
                  {schemaQuery.data && schemaQuery.data.labels.length === 0 ? (
                    <p className="rounded-2xl border border-dashed border-border/80 px-4 py-4 text-sm text-muted-foreground">
                      No labels available yet.
                    </p>
                  ) : null}
                </div>
              </section>

              <section>
                <div className="flex items-center justify-between">
                  <p className="text-sm font-semibold text-foreground">Relationship types</p>
                  <span className="text-xs uppercase tracking-[0.18em] text-muted-foreground">
                    {schemaQuery.data?.relationship_types.length ?? 0}
                  </span>
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  {schemaQuery.data?.relationship_types.map(({ type, count }) => (
                    <div
                      key={type}
                      className="rounded-full border border-border/80 bg-white/70 px-3 py-2 text-xs font-medium text-foreground"
                    >
                      {type} · {count}
                    </div>
                  ))}
                </div>
              </section>

              {schemaQuery.error ? (
                <p className="text-sm text-destructive">{schemaQuery.error.message}</p>
              ) : null}
            </div>
          </aside>

          <div className="space-y-4">
            <div className="grid gap-4 lg:grid-cols-2">
              <article className="rounded-[1.75rem] border border-border/70 bg-card/90 p-6 shadow-panel backdrop-blur">
                <p className="text-xs font-semibold uppercase tracking-[0.24em] text-primary">
                  Cypher query
                </p>
                <h3 className="mt-3 text-lg font-semibold">`POST /cypher`</h3>
                <p className="mt-3 text-sm leading-6 text-muted-foreground">
                  CodeMirror 6 drives the query surface with lightweight Cypher syntax
                  highlighting. One-click label scans reuse the same mutation path.
                </p>
                <div className="mt-5">
                  <CypherEditor
                    value={queryText}
                    onChange={setQueryText}
                    onRun={handleRunQuery}
                    isRunning={cypherMutation.isPending}
                  />
                </div>
                {cypherMutation.isPending ? (
                  <div className="mt-3 flex items-center gap-2 rounded-2xl border border-primary/15 bg-primary/5 px-4 py-3 text-sm text-primary">
                    <LoaderCircle className="h-4 w-4 animate-spin" />
                    <span>Submitting query to `/cypher`…</span>
                  </div>
                ) : null}
                {cypherMutation.error ? (
                  <QueryErrorPanel error={cypherMutation.error} queryText={queryText} />
                ) : null}
                <div className="mt-4 flex flex-wrap gap-3">
                  <Button
                    onClick={() => handleRunQuery(queryText)}
                    disabled={cypherMutation.isPending}
                  >
                    {cypherMutation.isPending ? (
                      <>
                        <LoaderCircle className="h-4 w-4 animate-spin" />
                        Running query…
                      </>
                    ) : (
                      'Run query'
                    )}
                  </Button>
                  <Button
                    variant="outline"
                    onClick={() => {
                      setQueryText('MATCH (n) RETURN n LIMIT 25')
                      handleRunQuery('MATCH (n) RETURN n LIMIT 25')
                    }}
                  >
                    Load sample query
                  </Button>
                  {activeLabelScan ? (
                    <div className="inline-flex items-center rounded-full border border-primary/20 bg-primary/10 px-3 py-2 text-xs font-semibold uppercase tracking-[0.2em] text-primary">
                      Active scan: {activeLabelScan}
                    </div>
                  ) : null}
                </div>
                <div className="mt-4 text-sm">
                  <p>
                    Rows:{' '}
                    <span className="font-medium text-foreground">
                      {cypherMutation.data?.rows.length ?? 0}
                    </span>
                  </p>
                  <p>
                    Graph nodes:{' '}
                    <span className="font-medium text-foreground">
                      {cypherMutation.data?.graph.nodes.length ?? 0}
                    </span>
                  </p>
                  {cypherMutation.data?.rows[0]?.[0] &&
                  isGraphNode(cypherMutation.data.rows[0][0]) ? (
                    <p className="mt-2 text-muted-foreground">
                      First node labels: {cypherMutation.data.rows[0][0].labels.join(', ')}
                    </p>
                  ) : null}
                  {cypherMutation.data?.rows[0]?.[1] &&
                  isGraphEdge(cypherMutation.data.rows[0][1]) ? (
                    <p className="mt-2 text-muted-foreground">
                      First edge type: {cypherMutation.data.rows[0][1].type}
                    </p>
                  ) : null}
                </div>
              </article>

              <article className="rounded-[1.75rem] border border-border/70 bg-card/90 p-6 shadow-panel backdrop-blur">
                <p className="text-xs font-semibold uppercase tracking-[0.24em] text-primary">
                  Node expand
                </p>
                <h3 className="mt-3 text-lg font-semibold">`GET /node/:id`</h3>
                <p className="mt-3 text-sm leading-6 text-muted-foreground">
                  Double-click a canvas node to fetch neighbors and merge them into the
                  current graph without duplicating existing records.
                </p>
                <div className="mt-5 flex gap-3">
                  <Button
                    variant="outline"
                    onClick={() => {
                      const firstNodeId = canvasNodes[0]?.id

                      if (firstNodeId) {
                        handleExpandNode(firstNodeId)
                      }
                    }}
                  >
                    Use sample node
                  </Button>
                  <Button
                    variant="ghost"
                    onClick={() => {
                      setSelectedNodeId(null)
                      nodeExpandMutation.reset()
                    }}
                  >
                    Clear
                  </Button>
                </div>
                <div className="mt-4 text-sm">
                  <p>
                    Selected node:{' '}
                    <span className="font-medium text-foreground">
                      {selectedNodeId ?? 'none'}
                    </span>
                  </p>
                  <p>
                    Adjacent nodes:{' '}
                    <span className="font-medium text-foreground">
                      {nodeExpandMutation.data?.nodes.length ?? 0}
                    </span>
                  </p>
                  <p>
                    Incident edges:{' '}
                    <span className="font-medium text-foreground">
                      {nodeExpandMutation.data?.edges.length ?? 0}
                    </span>
                  </p>
                  {nodeExpandMutation.isPending ? (
                    <p className="mt-2 text-muted-foreground">Expanding node neighborhood…</p>
                  ) : null}
                  {nodeExpandMutation.error ? (
                    <p className="mt-2 text-destructive">{nodeExpandMutation.error.message}</p>
                  ) : null}
                </div>
              </article>
            </div>

            <GraphCanvas
              nodes={canvasNodes}
              edges={canvasEdges}
              selectedNodeId={inspectedNodeId}
              onNodeSelect={setInspectedNodeId}
              onNodeDoubleClick={handleExpandNode}
              onCanvasClear={() => setInspectedNodeId(null)}
            />
          </div>

          <article className="rounded-[1.75rem] border border-border/70 bg-[#16373a] p-6 text-white shadow-panel">
            <p className="text-xs font-semibold uppercase tracking-[0.24em] text-white/60">
              Node inspector
            </p>
            <h3 className="mt-3 text-lg font-semibold">
              Selection details
            </h3>
            <div className="mt-4 space-y-4 text-sm text-white/78">
              {inspectedNode ? (
                <>
                  <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
                    <p className="text-xs font-semibold uppercase tracking-[0.2em] text-white/55">
                      Node id
                    </p>
                    <p className="mt-2 break-all font-medium text-white">
                      {inspectedNode.id}
                    </p>
                  </div>

                  <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
                    <p className="text-xs font-semibold uppercase tracking-[0.2em] text-white/55">
                      Labels
                    </p>
                    <div className="mt-3 flex flex-wrap gap-2">
                      {inspectedNode.labels.map((label) => (
                        <span
                          key={label}
                          className="rounded-full border border-white/10 bg-white/10 px-3 py-1 text-xs font-medium text-white"
                        >
                          {label}
                        </span>
                      ))}
                    </div>
                  </div>

                  <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
                    <p className="text-xs font-semibold uppercase tracking-[0.2em] text-white/55">
                      Properties
                    </p>
                    <dl className="mt-3 space-y-3">
                      {Object.entries(inspectedNode.properties).map(([key, value]) => (
                        <div
                          key={key}
                          className="border-b border-white/10 pb-3 last:border-b-0 last:pb-0"
                        >
                          <dt className="text-xs uppercase tracking-[0.18em] text-white/55">
                            {key}
                          </dt>
                          <dd className="mt-1 break-words font-medium text-white">
                            {formatJsonValue(value)}
                          </dd>
                        </div>
                      ))}
                    </dl>
                  </div>
                </>
              ) : (
                <div className="rounded-2xl border border-dashed border-white/15 bg-white/5 p-5">
                  <p className="font-medium text-white">No node selected</p>
                  <p className="mt-2 leading-6 text-white/70">
                    Click a node in the graph canvas to inspect its labels and properties.
                    Double-click a node to expand its neighborhood. Click the canvas
                    background to clear the selection.
                  </p>
                </div>
              )}

              <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
                <p className="font-medium text-white">Current graph payload</p>
                <p className="mt-2">Nodes: {canvasNodes.length}</p>
                <p>Edges: {canvasEdges.length}</p>
              </div>

              <div className="flex gap-3">
                <Button
                  variant="outline"
                  className="border-white/20 bg-transparent text-white hover:bg-white/10 hover:text-white"
                  onClick={() => setInspectedNodeId(canvasNodes[0]?.id ?? null)}
                >
                  Select first node
                </Button>
                <Button
                  variant="ghost"
                  className="text-white hover:bg-white/10 hover:text-white"
                  onClick={() => setInspectedNodeId(null)}
                >
                  Clear selection
                </Button>
              </div>
            </div>
          </article>
        </div>
      </section>
    </div>
  )
}

export default App

function mergeExpandedGraph(
  currentGraph: GraphResult,
  expandedGraph: NodeNeighborsResult,
): GraphResult {
  const nodeMap = new Map<string, GraphNode>()
  const edgeMap = new Map<string, GraphEdge>()

  for (const node of currentGraph.nodes) {
    nodeMap.set(node.id, node)
  }

  for (const node of [expandedGraph.node, ...expandedGraph.nodes]) {
    nodeMap.set(node.id, node)
  }

  for (const edge of currentGraph.edges) {
    edgeMap.set(edge.id, edge)
  }

  for (const edge of expandedGraph.edges) {
    edgeMap.set(edge.id, edge)
  }

  return {
    nodes: [...nodeMap.values()],
    edges: [...edgeMap.values()],
  }
}

function formatJsonValue(value: unknown) {
  if (typeof value === 'string') {
    return value
  }

  return JSON.stringify(value)
}

function escapeCypherIdentifier(identifier: string) {
  return `\`${identifier.replaceAll('`', '``')}\``
}

function QueryErrorPanel({
  error,
  queryText,
}: {
  error: Error
  queryText: string
}) {
  const apiError = error instanceof ApiError ? error : null
  const lineNumber = apiError?.line ?? null
  const columnNumber = apiError?.col ?? null
  const queryLines = queryText.split('\n')
  const offendingLine =
    lineNumber && lineNumber >= 1 && lineNumber <= queryLines.length
      ? queryLines[lineNumber - 1]
      : null
  const caretIndent = offendingLine && columnNumber && columnNumber > 0
    ? ' '.repeat(Math.max(0, columnNumber - 1))
    : ''

  return (
    <div className="mt-3 rounded-[1.35rem] border border-destructive/30 bg-[linear-gradient(180deg,_rgba(254,242,242,0.96),_rgba(255,250,250,0.98))] p-4 text-sm shadow-inner">
      <div className="flex items-start gap-3">
        <div className="mt-0.5 h-2.5 w-2.5 rounded-full bg-destructive" />
        <div className="min-w-0 flex-1">
          <p className="font-semibold text-destructive">Query failed</p>
          <p className="mt-1 leading-6 text-[#7f1d1d]">{error.message}</p>
          {lineNumber ? (
            <p className="mt-2 text-xs font-semibold uppercase tracking-[0.2em] text-[#991b1b]/80">
              Line {lineNumber}
              {columnNumber ? `, column ${columnNumber}` : ''}
            </p>
          ) : null}
        </div>
      </div>

      {offendingLine !== null ? (
        <div className="mt-4 overflow-hidden rounded-2xl border border-destructive/20 bg-[#2b1616] text-[13px] text-white shadow-inner">
          <div className="flex border-b border-white/10 px-4 py-2 text-[11px] font-semibold uppercase tracking-[0.22em] text-white/55">
            Offending line
          </div>
          <pre className="overflow-x-auto px-4 py-4 font-mono leading-6">
            <span className="text-white/45">{String(lineNumber).padStart(3, ' ')}</span>
            <span className="ml-3 text-[#fecaca]">{offendingLine}</span>
            {columnNumber ? (
              <>
                {'\n'}
                <span className="text-transparent">{String(lineNumber).padStart(3, ' ')}</span>
                <span className="ml-3 text-[#fb7185]">
                  {caretIndent}
                  ^
                </span>
              </>
            ) : null}
          </pre>
        </div>
      ) : null}
    </div>
  )
}
