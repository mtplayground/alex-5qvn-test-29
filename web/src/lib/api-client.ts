import type {
  ApiErrorPayload,
  CypherRequest,
  NodeNeighborsResult,
  QueryResult,
  SchemaCatalog,
} from '@/lib/api-types'

export class ApiError extends Error {
  readonly status: number
  readonly line: number | null
  readonly col: number | null

  constructor(status: number, payload: ApiErrorPayload) {
    super(payload.error)
    this.name = 'ApiError'
    this.status = status
    this.line = payload.line
    this.col = payload.col
  }
}

async function request<T>(
  input: string,
  init?: RequestInit,
): Promise<T> {
  const response = await fetch(input, {
    headers: {
      Accept: 'application/json',
      ...(init?.body ? { 'Content-Type': 'application/json' } : {}),
      ...init?.headers,
    },
    ...init,
  })

  if (!response.ok) {
    const payload = (await response.json()) as ApiErrorPayload
    throw new ApiError(response.status, payload)
  }

  return (await response.json()) as T
}

export async function runCypherQuery(
  payload: CypherRequest,
): Promise<QueryResult> {
  return request<QueryResult>('/cypher', {
    method: 'POST',
    body: JSON.stringify(payload),
  })
}

export async function fetchSchemaCatalog(): Promise<SchemaCatalog> {
  return request<SchemaCatalog>('/schema')
}

export async function fetchNodeNeighbors(
  nodeId: string,
): Promise<NodeNeighborsResult> {
  return request<NodeNeighborsResult>(`/node/${nodeId}`)
}
