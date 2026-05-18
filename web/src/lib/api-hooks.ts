import {
  useMutation,
  useQuery,
  type UseMutationOptions,
  type UseQueryOptions,
} from '@tanstack/react-query'

import {
  fetchNodeNeighbors,
  fetchSchemaCatalog,
  runCypherQuery,
} from '@/lib/api-client'
import type {
  CypherRequest,
  NodeNeighborsResult,
  QueryResult,
  SchemaCatalog,
} from '@/lib/api-types'

export const queryKeys = {
  schema: ['schema'] as const,
  node: (nodeId: string) => ['node', nodeId] as const,
}

export function useSchemaQuery(
  options?: Omit<
    UseQueryOptions<SchemaCatalog, Error, SchemaCatalog, typeof queryKeys.schema>,
    'queryKey' | 'queryFn'
  >,
) {
  return useQuery({
    queryKey: queryKeys.schema,
    queryFn: fetchSchemaCatalog,
    ...options,
  })
}

export function useNodeNeighborsQuery(
  nodeId: string | null,
  options?: Omit<
    UseQueryOptions<
      NodeNeighborsResult,
      Error,
      NodeNeighborsResult,
      ReturnType<typeof queryKeys.node>
    >,
    'queryKey' | 'queryFn'
  >,
) {
  return useQuery({
    queryKey: queryKeys.node(nodeId ?? 'unknown'),
    queryFn: () => fetchNodeNeighbors(nodeId ?? ''),
    enabled: Boolean(nodeId),
    ...options,
  })
}

export function useCypherMutation(
  options?: UseMutationOptions<QueryResult, Error, CypherRequest>,
) {
  return useMutation({
    mutationFn: runCypherQuery,
    ...options,
  })
}
