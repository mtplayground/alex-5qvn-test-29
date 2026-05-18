import { useState } from 'react'
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
  const schemaQuery = useSchemaQuery()
  const nodeQuery = useNodeNeighborsQuery(selectedNodeId)
  const cypherMutation = useCypherMutation()
  const canvasNodes = nodeQuery.data
    ? [nodeQuery.data.node, ...nodeQuery.data.nodes]
    : (cypherMutation.data?.graph.nodes ?? [])
  const canvasEdges = nodeQuery.data
    ? nodeQuery.data.edges
    : (cypherMutation.data?.graph.edges ?? [])

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
          <GraphCanvas nodes={canvasNodes} edges={canvasEdges} />

          <article className="rounded-[1.75rem] border border-border/70 bg-[#16373a] p-6 text-white shadow-panel">
            <p className="text-xs font-semibold uppercase tracking-[0.24em] text-white/60">
              Canvas behavior
            </p>
            <h3 className="mt-3 text-lg font-semibold">
              Cytoscape + `cose-bilkent`
            </h3>
            <div className="mt-4 space-y-4 text-sm text-white/78">
              <p>
                The canvas accepts typed `nodes` and `edges`, runs a force-directed layout,
                and leaves dragging, panning, and zooming enabled by default.
              </p>
              <p>
                Node color is derived from the label set, while edge color is derived from
                the relationship type so the visual system stays stable across fetches.
              </p>
              <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
                <p className="font-medium text-white">Current graph payload</p>
                <p className="mt-2">Nodes: {canvasNodes.length}</p>
                <p>Edges: {canvasEdges.length}</p>
              </div>
            </div>
          </article>
        </div>
      </section>
    </div>
  )
}

export default App
