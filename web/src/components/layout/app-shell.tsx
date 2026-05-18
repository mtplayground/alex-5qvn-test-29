import { ArrowRight, type LucideIcon } from 'lucide-react'

import { Button } from '@/components/ui/button'

type Highlight = {
  title: string
  description: string
  icon: LucideIcon
}

type AppShellProps = {
  badge: string
  title: string
  description: string
  highlights: readonly Highlight[]
}

export function AppShell({
  badge,
  title,
  description,
  highlights,
}: AppShellProps) {
  return (
    <main className="relative overflow-hidden">
      <div className="absolute inset-x-0 top-0 h-64 bg-[radial-gradient(circle_at_top,_hsl(197_79%_34%_/_0.16),_transparent_65%)]" />
      <div className="container relative flex min-h-screen flex-col py-10 sm:py-16">
        <header className="flex items-center justify-between gap-4">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.32em] text-primary">
              ZeroClaw
            </p>
            <h1 className="mt-3 text-2xl font-semibold tracking-tight sm:text-3xl">
              Alex graph playground
            </h1>
          </div>
          <div className="rounded-full border border-border/70 bg-white/80 px-4 py-2 text-xs font-medium uppercase tracking-[0.24em] text-muted-foreground shadow-panel backdrop-blur">
            {badge}
          </div>
        </header>

        <section className="mt-10 grid gap-6 lg:grid-cols-[1.3fr_0.9fr]">
          <article className="rounded-[2rem] border border-border/70 bg-card/85 p-8 shadow-panel backdrop-blur animate-fade-in sm:p-10">
            <span className="inline-flex rounded-full bg-primary/10 px-4 py-1 text-sm font-medium text-primary">
              Ready for query and canvas flows
            </span>
            <h2 className="mt-6 max-w-2xl text-4xl font-semibold leading-tight tracking-tight sm:text-5xl">
              {title}
            </h2>
            <p className="mt-5 max-w-2xl text-base leading-7 text-muted-foreground sm:text-lg">
              {description}
            </p>
            <div className="mt-8 flex flex-col gap-3 sm:flex-row">
              <Button size="lg">Open local workspace</Button>
              <Button size="lg" variant="outline">
                Backend proxy on :3000
              </Button>
            </div>
          </article>

          <aside
            className="rounded-[2rem] border border-border/70 bg-[#16373a] p-8 text-white shadow-panel animate-fade-in"
            id="backend-health"
          >
            <p className="text-xs font-semibold uppercase tracking-[0.3em] text-white/60">
              Dev routing
            </p>
            <div className="mt-6 space-y-4 text-sm">
              <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
                <p className="font-medium">Vite</p>
                <p className="mt-1 text-white/70">
                  Serves the frontend on <code>http://localhost:3000</code>
                </p>
              </div>
              <div className="rounded-2xl border border-white/10 bg-white/5 p-4">
                <p className="font-medium">Rust API proxy</p>
                <p className="mt-1 text-white/70">
                  Forwards <code>/healthz</code>, <code>/cypher</code>, <code>/schema</code>, and <code>/node</code> to the backend on <code>:8080</code>
                </p>
              </div>
            </div>
          </aside>
        </section>

        <section className="mt-8 grid gap-4 md:grid-cols-3">
          {highlights.map(({ title: itemTitle, description: itemDescription, icon: Icon }) => (
            <article
              key={itemTitle}
              className="rounded-[1.75rem] border border-border/70 bg-card/85 p-6 shadow-panel backdrop-blur animate-fade-in"
            >
              <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-accent/15 text-accent">
                <Icon className="h-6 w-6" />
              </div>
              <h3 className="mt-5 text-lg font-semibold">{itemTitle}</h3>
              <p className="mt-3 text-sm leading-6 text-muted-foreground">
                {itemDescription}
              </p>
            </article>
          ))}
        </section>

        <footer className="mt-auto pt-10">
          <div className="flex flex-col gap-3 rounded-[1.75rem] border border-border/70 bg-white/70 p-5 text-sm text-muted-foreground shadow-panel backdrop-blur sm:flex-row sm:items-center sm:justify-between">
            <p>Scaffolded with Vite, React, Tailwind CSS, and shadcn/ui conventions.</p>
            <a
              className="inline-flex items-center gap-2 font-medium text-primary transition-colors hover:text-primary/80"
              href="#backend-health"
            >
              Check backend health
              <ArrowRight className="h-4 w-4" />
            </a>
          </div>
        </footer>
      </div>
    </main>
  )
}
