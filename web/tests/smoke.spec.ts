import { expect, test, type Page } from '@playwright/test'

type GraphNodePosition = {
  id: string
  label: string
  renderedX: number
  renderedY: number
}

test('graph playground smoke flow', async ({ page }) => {
  await page.goto('/')

  await setEditorQuery(page, 'MATCH (n:Station) RETURN n LIMIT 25')
  await page.getByRole('button', { name: 'Run query' }).click()

  const initialPositions = await waitForGraphNodes(page, (positions) => positions.length >= 25)
  expect(initialPositions.length).toBeGreaterThanOrEqual(25)

  const draggedNode = initialPositions[0]

  await page.evaluate(
    ({ nodeId, deltaX, deltaY }) =>
      window.__graphCanvasDebug?.dragNodeBy(nodeId, deltaX, deltaY),
    {
      nodeId: draggedNode.id,
      deltaX: 90,
      deltaY: 55,
    },
  )

  const movedPositions = await waitForGraphNodes(page, (positions) => {
    const movedNode = positions.find((position) => position.id === draggedNode.id)

    if (!movedNode) {
      return false
    }

    return (
      Math.abs(movedNode.renderedX - draggedNode.renderedX) > 1 ||
      Math.abs(movedNode.renderedY - draggedNode.renderedY) > 1
    )
  })

  const movedNode = movedPositions.find((position) => position.id === draggedNode.id)
  expect(movedNode).toBeDefined()
  expect(
    Math.abs((movedNode?.renderedX ?? draggedNode.renderedX) - draggedNode.renderedX) > 1 ||
      Math.abs((movedNode?.renderedY ?? draggedNode.renderedY) - draggedNode.renderedY) > 1,
  ).toBeTruthy()

  const testStopName = `TestStop-${Date.now()}`
  await setEditorQuery(page, `CREATE (n:Station {name:"${testStopName}"})`)
  await page.getByRole('button', { name: 'Run query' }).click()

  const mutationPositions = await waitForGraphNodes(page, (positions) =>
    positions.some((position) => position.label === testStopName),
  )

  expect(mutationPositions.some((position) => position.label === testStopName)).toBeTruthy()
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

  return (await readGraphNodePositions(page))
}
