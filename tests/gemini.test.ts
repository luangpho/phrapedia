import { describe, it, expect } from 'vitest'
import { unwrapGeminiOutput } from '../server/utils/gemini'

describe('unwrapGeminiOutput', () => {
  it('unwraps the JSON envelope', () => {
    const out = JSON.stringify({ response: '## 結果\nワット・ラカン', stats: { models: {} } })
    expect(unwrapGeminiOutput(out)).toBe('## 結果\nワット・ラカン')
  })

  it('returns raw text unchanged (trimmed)', () => {
    expect(unwrapGeminiOutput('  plain markdown answer\n')).toBe('plain markdown answer')
  })

  it('tolerates log lines before the JSON', () => {
    const out = 'Loaded cached credentials.\n[INFO] something\n' + JSON.stringify({ response: 'OK' }, null, 2)
    expect(unwrapGeminiOutput(out)).toBe('OK')
  })

  it('treats text with braces that is not JSON as raw text', () => {
    const text = 'answer with {braces} inside'
    expect(unwrapGeminiOutput(text)).toBe(text)
  })

  it('throws on an error envelope (object)', () => {
    const out = JSON.stringify({ error: { type: 'ApiError', message: 'quota exceeded', code: 429 } })
    expect(() => unwrapGeminiOutput(out)).toThrowError('quota exceeded')
    try {
      unwrapGeminiOutput(out)
    } catch (err) {
      expect((err as { statusCode?: number }).statusCode).toBe(502)
    }
  })

  it('throws on an error envelope (string)', () => {
    expect(() => unwrapGeminiOutput(JSON.stringify({ error: 'auth failed' }))).toThrowError('auth failed')
  })
})
