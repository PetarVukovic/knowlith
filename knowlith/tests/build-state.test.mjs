import test from 'node:test'
import assert from 'node:assert/strict'
import { buildState } from '../src/lib/buildState.ts'

const idle = {stage: 'idle', lines: [], held: null}
test('an empty idle queue is not a completed company brain', () => {
  assert.equal(buildState(idle, 0), 'empty')
})
test('a failed build cannot be mistaken for success', () => {
  assert.equal(buildState({...idle, lines: [{state: 'failed'}]}, 3), 'attention')
})
test('paused work has a paused state even when discoveries exist', () => {
  assert.equal(buildState({...idle, stage: 'held'}, 3), 'paused')
})
test('only an idle successful build with findings is ready to review', () => {
  assert.equal(buildState(idle, 3), 'ready')
  assert.equal(buildState({...idle, stage: 'thinking'}, 3), 'working')
  assert.equal(buildState(null, 3), 'connecting')
})
