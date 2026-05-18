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

  const payload = await readJsonBody(response)

  if (!response.ok) {
    throw new ApiError(
      response.status,
      isApiErrorPayload(payload)
        ? payload
        : {
            error:
              response.statusText ||
              'Request failed before a structured error payload could be read.',
            line: null,
            col: null,
          },
    )
  }

  return payload as T
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

async function readJsonBody(response: Response): Promise<unknown> {
  const responseText = await response.text()

  if (!responseText.trim()) {
    return null
  }

  try {
    return JSON.parse(responseText) as unknown
  } catch {
    return {
      error: responseText,
      line: null,
      col: null,
    } satisfies ApiErrorPayload
  }
}

function isApiErrorPayload(value: unknown): value is ApiErrorPayload {
  return (
    typeof value === 'object' &&
    value !== null &&
    'error' in value &&
    typeof value.error === 'string' &&
    'line' in value &&
    'col' in value
  )
}
