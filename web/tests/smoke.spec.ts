import { expect, test, type Page } from '@playwright/test'

type GraphNodePosition = {
  id: string
  label: string
  renderedX: number
  renderedY: number
}

test('graph playground smoke flow', async ({ page }) => {
  await page.goto('/')

  await setEditorQuery(page, 'MATCH (n)-[r]-(m) RETURN n, r, m LIMIT 30')
  await page.getByRole('button', { name: 'Run query' }).click()
  await expect(page.getByText(/First edge type:/)).toBeVisible()

  const relationPositions = await waitForGraphNodes(
    page,
    (positions) => positions.length > 0,
  )
  expect(relationPositions.length).toBeGreaterThan(0)
  await expect
    .poll(async () => (await readVisibleNodeColors(page)).length)
    .toBeGreaterThanOrEqual(2)

  await page.getByRole('button', { name: /^Station\b/ }).click()
  await expect(page.getByText('Active scan: Station')).toBeVisible()
  await waitForGraphNodes(page, (positions) => positions.length > 0)

  await setEditorQuery(page, 'MATCH (n:Station) RETURN n LIMIT 5')
  await page.getByRole('button', { name: 'Run query' }).click()
  await expect(page.getByText('Rows: 5')).toBeVisible()

  const stationPositions = await waitForGraphNodes(
    page,
    (positions) => positions.length === 5,
  )
  expect(stationPositions).toHaveLength(5)
  await expect(page.getByText('Nodes: 5', { exact: true })).toBeVisible()
  await expect(page.getByText('Edges: 0', { exact: true })).toBeVisible()

  await page.getByRole('button', { name: 'Use sample node' }).click()
  await expect
    .poll(async () => readPanelMetric(page, 'Adjacent nodes'))
    .toBeGreaterThan(0)
  await expect
    .poll(async () => readPanelMetric(page, 'Incident edges'))
    .toBeGreaterThan(0)

  const clearPoint = await findClearCanvasPoint(page)
  await page
    .getByTestId('graph-viewport')
    .click({ position: clearPoint, force: true })

  await expect.poll(async () => readPanelMetric(page, 'Adjacent nodes')).toBe(0)
  await expect.poll(async () => readPanelMetric(page, 'Incident edges')).toBe(0)
})

async function setEditorQuery(page: Page, query: string) {
  const editor = page.getByTestId('cypher-editor').locator('.cm-content')

  await editor.click()
  await page.keyboard.press('Control+A')
  await page.keyboard.press('Backspace')
  await page.keyboard.type(query)
}

async function readGraphNodePositions(page: Page): Promise<GraphNodePosition[]> {
  const payload = (await page.getByTestId('graph-node-positions').textContent()) ?? '[]'

  return JSON.parse(payload) as GraphNodePosition[]
}

async function waitForGraphNodes(
  page: Page,
  predicate: (positions: GraphNodePosition[]) => boolean,
) {
  await expect
    .poll(async () => {
      const positions = await readGraphNodePositions(page)

      return predicate(positions) ? positions : null
    })
    .not.toBeNull()

  return await readGraphNodePositions(page)
}

async function readVisibleNodeColors(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const tones = window.__graphCanvasDebug?.getNodeTones() ?? []
    return [...new Set(tones.map((entry) => entry.tone).filter(Boolean))]
  })
}

async function findClearCanvasPoint(page: Page): Promise<{ x: number; y: number }> {
  return page.evaluate(() => {
    const viewport = document.querySelector<HTMLElement>('[data-testid="graph-viewport"]')
    const positions = window.__graphCanvasDebug?.getNodePositions() ?? []

    if (!viewport) {
      return { x: 12, y: 12 }
    }

    const width = viewport.clientWidth
    const height = viewport.clientHeight
    const candidates = [
      { x: 12, y: 12 },
      { x: width - 12, y: 12 },
      { x: 12, y: height - 12 },
      { x: width - 12, y: height - 12 },
      { x: Math.floor(width / 2), y: 12 },
      { x: 12, y: Math.floor(height / 2) },
    ]

    for (const candidate of candidates) {
      const overlapsNode = positions.some((position) => {
        const deltaX = position.renderedX - candidate.x
        const deltaY = position.renderedY - candidate.y

        return Math.hypot(deltaX, deltaY) < 45
      })

      if (!overlapsNode) {
        return candidate
      }
    }

    return { x: 12, y: 12 }
  })
}

async function readPanelMetric(page: Page, label: string): Promise<number> {
  const line = page.getByText(new RegExp(`^${escapeRegExp(label)}:\\s`)).first()
  const text = (await line.textContent()) ?? ''
  const match = text.match(/:\s*(\d+)/)

  return match ? Number(match[1]) : Number.NaN
}

function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}
