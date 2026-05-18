import { Activity, DatabaseZap, Network } from 'lucide-react'

import { AppShell } from '@/components/layout/app-shell'

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
  return (
    <AppShell
      badge="Frontend scaffold"
      title="Graph playground workspace"
      description="A Vite + React + Tailwind shell prepared for graph exploration, query editing, and schema-aware API workflows."
      highlights={highlights}
    />
  )
}

export default App
