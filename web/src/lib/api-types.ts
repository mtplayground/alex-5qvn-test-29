export type JsonPrimitive = string | number | boolean | null
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue }

export type Properties = Record<string, JsonValue>

export type GraphNode = {
  id: string
  labels: string[]
  properties: Properties
}

export type GraphEdge = {
  id: string
  start_id: string
  end_id: string
  type: string
  properties: Properties
}

export type GraphResult = {
  nodes: GraphNode[]
  edges: GraphEdge[]
}

export type QueryValue = GraphNode | GraphEdge | JsonValue

export type QueryResult = {
  columns: string[]
  rows: QueryValue[][]
  graph: GraphResult
}

export type LabelCount = {
  label: string
  count: number
}

export type RelationshipTypeCount = {
  type: string
  count: number
}

export type SchemaCatalog = {
  labels: LabelCount[]
  relationship_types: RelationshipTypeCount[]
}

export type NodeNeighborsResult = {
  node: GraphNode
  edges: GraphEdge[]
  nodes: GraphNode[]
}

export type CypherRequest = {
  query: string
  params?: JsonValue
}

export type ApiErrorPayload = {
  error: string
  line: number | null
  col: number | null
}

export function isGraphNode(value: QueryValue): value is GraphNode {
  return (
    typeof value === 'object' &&
    value !== null &&
    !Array.isArray(value) &&
    'id' in value &&
    'labels' in value &&
    'properties' in value
  )
}

export function isGraphEdge(value: QueryValue): value is GraphEdge {
  return (
    typeof value === 'object' &&
    value !== null &&
    !Array.isArray(value) &&
    'id' in value &&
    'start_id' in value &&
    'end_id' in value &&
    'type' in value &&
    'properties' in value
  )
}
