import { useEffect, useState } from 'react'
import { Activity, DatabaseZap, Network } from 'lucide-react'

import { GraphCanvas } from '@/components/graph/graph-canvas'
import { AppShell } from '@/components/layout/app-shell'
import { Button } from '@/components/ui/button'
import { useCypherMutation, useNodeNeighborsQuery, useSchemaQuery } from '@/lib/api-hooks'
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
  const schemaQuery = useSchemaQuery()
  const nodeQuery = useNodeNeighborsQuery(selectedNodeId)
  const cypherMutation = useCypherMutation()
  const canvasNodes = nodeQuery.data
    ? [nodeQuery.data.node, ...nodeQuery.data.nodes]
    : (cypherMutation.data?.graph.nodes ?? [])
  const canvasEdges = nodeQuery.data
    ? nodeQuery.data.edges
    : (cypherMutation.data?.graph.edges ?? [])
  const inspectedNode =
    canvasNodes.find((node) => node.id === inspectedNodeId) ?? null

  useEffect(() => {
    if (!inspectedNodeId) {
      return
    }

    const hasSelectedNode = canvasNodes.some((node) => node.id === inspectedNodeId)

    if (!hasSelectedNode) {
      setInspectedNodeId(null)
    }
  }, [canvasNodes, inspectedNodeId])

  return (
    <div>
      <AppShell
        badge="API client ready"
        title="Graph playground workspace"
        description="A typed frontend shell prepared for schema reads, node expansion, and Cypher execution over the Rust graph API."
        highlights={highlights}
      />

      <section className="container -mt-6 pb-12">
        <div className="grid gap-4 lg:grid-cols-3">
          <article className="rounded-[1.75rem] border border-border/70 bg-card/90 p-6 shadow-panel backdrop-blur">
            <p className="text-xs font-semibold uppercase tracking-[0.24em] text-primary">
              Schema query
            </p>
            <h3 className="mt-3 text-lg font-semibold">`GET /schema`</h3>
            <p className="mt-3 text-sm leading-6 text-muted-foreground">
              Labels and relationship types are fetched through a typed Query hook.
            </p>
            <div className="mt-5 space-y-2 text-sm">
              <p>
                Labels:{' '}
                <span className="font-medium text-foreground">
                  {schemaQuery.data?.labels.length ?? 0}
                </span>
              </p>
              <p>
                Relationship types:{' '}
                <span className="font-medium text-foreground">
                  {schemaQuery.data?.relationship_types.length ?? 0}
                </span>
              </p>
              {schemaQuery.error ? (
                <p className="text-destructive">{schemaQuery.error.message}</p>
              ) : null}
            </div>
          </article>

          <article className="rounded-[1.75rem] border border-border/70 bg-card/90 p-6 shadow-panel backdrop-blur">
            <p className="text-xs font-semibold uppercase tracking-[0.24em] text-primary">
              Cypher query
            </p>
            <h3 className="mt-3 text-lg font-semibold">`POST /cypher`</h3>
            <p className="mt-3 text-sm leading-6 text-muted-foreground">
              Mutations and reads share one typed result envelope with graph rows.
            </p>
            <Button
              className="mt-5"
              onClick={() =>
                cypherMutation.mutate({
                  query: 'MATCH (n) RETURN n LIMIT 1',
                })
              }
            >
              Run sample query
            </Button>
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
              {cypherMutation.data?.rows[0]?.[0] && isGraphNode(cypherMutation.data.rows[0][0]) ? (
                <p className="mt-2 text-muted-foreground">
                  First node labels: {cypherMutation.data.rows[0][0].labels.join(', ')}
                </p>
              ) : null}
              {cypherMutation.data?.rows[0]?.[1] && isGraphEdge(cypherMutation.data.rows[0][1]) ? (
                <p className="mt-2 text-muted-foreground">
                  First edge type: {cypherMutation.data.rows[0][1].type}
                </p>
              ) : null}
              {cypherMutation.error ? (
                <p className="mt-2 text-destructive">{cypherMutation.error.message}</p>
              ) : null}
            </div>
          </article>

          <article className="rounded-[1.75rem] border border-border/70 bg-card/90 p-6 shadow-panel backdrop-blur">
            <p className="text-xs font-semibold uppercase tracking-[0.24em] text-primary">
              Node expand
            </p>
            <h3 className="mt-3 text-lg font-semibold">`GET /node/:id`</h3>
            <p className="mt-3 text-sm leading-6 text-muted-foreground">
              Use the hook once a canvas node is selected. This panel wires the typed response.
            </p>
            <div className="mt-5 flex gap-3">
              <Button
                variant="outline"
                onClick={() => setSelectedNodeId(cypherMutation.data?.graph.nodes[0]?.id ?? null)}
              >
                Use sample node
              </Button>
              <Button variant="ghost" onClick={() => setSelectedNodeId(null)}>
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
                  {nodeQuery.data?.nodes.length ?? 0}
                </span>
              </p>
              <p>
                Incident edges:{' '}
                <span className="font-medium text-foreground">
                  {nodeQuery.data?.edges.length ?? 0}
                </span>
              </p>
              {nodeQuery.error ? (
                <p className="mt-2 text-destructive">{nodeQuery.error.message}</p>
              ) : null}
            </div>
          </article>
        </div>

        <div className="mt-4 grid gap-4 lg:grid-cols-[1.4fr_0.6fr]">
          <GraphCanvas
            nodes={canvasNodes}
            edges={canvasEdges}
            selectedNodeId={inspectedNodeId}
            onNodeSelect={setInspectedNodeId}
            onCanvasClear={() => setInspectedNodeId(null)}
          />

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
                    Click the canvas background to clear the selection.
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

function formatJsonValue(value: unknown) {
  if (typeof value === 'string') {
    return value
  }

  return JSON.stringify(value)
}
