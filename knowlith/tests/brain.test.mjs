import test from 'node:test'
import assert from 'node:assert/strict'
import { layoutBrainNodes } from '../src/lib/brainGraph.ts'

test('new findings do not move knowledge the owner was inspecting', () => {
  const nodes = [{id: 'rule:b', kind: 'rule'}, {id: 'rule:c', kind: 'rule'}, {id: 'doc:a', kind: 'document'}]
  const before = layoutBrainNodes(nodes, [])
  const after = layoutBrainNodes([...nodes, {id: 'rule:a', kind: 'rule'}, {id: 'doc:b', kind: 'document'}], [])
  for (const node of nodes) assert.deepEqual(after.get(node.id), before.get(node.id))
})

test('graph coordinates stay finite for a large mixed company', () => {
  const nodes = Array.from({length: 5000}, (_, i) => ({id: `rule:${i}`, kind: ['rule','process','fact','document'][i % 4]}))
  const positions = layoutBrainNodes(nodes, [])
  assert.equal(positions.size, 5000)
  for (const p of positions.values()) assert.ok([p.x,p.y,p.z].every(Number.isFinite))
})
