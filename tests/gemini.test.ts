import { describe, it, expect } from 'vitest'
import { unwrapGeminiOutput, buildCliInvocation, buildResearchPrompt } from '../server/utils/gemini'

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

describe('unwrapGeminiOutput (Antigravity CLI envelope)', () => {
  it('unwraps the agy envelope', () => {
    const out = JSON.stringify({ conversation_id: 'c1', status: 'success', response: '## 結果', usage: {} })
    expect(unwrapGeminiOutput(out)).toBe('## 結果')
  })

  it('keeps a partial response and notes an abnormal finished_reason', () => {
    const out = JSON.stringify({ conversation_id: 'c1', status: 'error', response: '途中まで', finished_reason: 'print_timeout', error: { message: 'timeout' } })
    expect(unwrapGeminiOutput(out)).toContain('途中まで')
    expect(unwrapGeminiOutput(out)).toContain('print_timeout')
  })
})

describe('buildCliInvocation', () => {
  it('puts the prompt in -p for Antigravity and does not use stdin', () => {
    const { args, stdin } = buildCliInvocation('antigravity', { model: 'flash', extraArgs: [], approvalMode: 'default', prompt: 'Q' })
    expect(stdin).toBeUndefined()
    expect(args).toEqual(['--model', 'flash', '--output-format', 'json', '-p', 'Q'])
  })

  it('maps approval modes to agy flags', () => {
    expect(buildCliInvocation('antigravity', { model: '', extraArgs: [], approvalMode: 'auto_edit', prompt: 'Q' }).args).toContain('accept-edits')
    expect(buildCliInvocation('antigravity', { model: '', extraArgs: [], approvalMode: 'yolo', prompt: 'Q' }).args).toContain('--dangerously-skip-permissions')
  })

  it('does not duplicate flags given in extra args', () => {
    const { args } = buildCliInvocation('antigravity', { model: 'flash', extraArgs: ['--model', 'pro', '--output-format', 'json'], approvalMode: 'default', prompt: 'Q' })
    expect(args.filter(a => a === '--model')).toHaveLength(1)
    expect(args.filter(a => a === '--output-format')).toHaveLength(1)
  })

  it('sends the prompt on stdin for Gemini CLI with a -p trailer', () => {
    const { args, stdin } = buildCliInvocation('gemini-cli', { model: 'flash', extraArgs: [], approvalMode: 'default', prompt: 'Q' })
    expect(stdin).toBe('Q')
    expect(args.slice(0, 2)).toEqual(['-m', 'flash'])
    expect(args).toContain('--approval-mode')
    expect(args).toContain('--skip-trust')
    expect(args[args.length - 2]).toBe('-p')
  })
})

describe('buildResearchPrompt', () => {
  it('adds mandatory image-reading instructions and the reference OCR text when images are attached', () => {
    const p = buildResearchPrompt('この画像のプラクルアンは何か', { images: ['images/amulet.jpg'], ocrText: 'วัดระฆัง 2500' })
    expect(p).toContain('image-1')
    expect(p).toContain('images/amulet.jpg')
    expect(p).toContain('必ず自分で開いて確認')
    expect(p).toContain('拡大')
    expect(p).toContain('参考: 事前に読み取られた文字')
    expect(p).toContain('วัดระฆัง 2500')
    expect(p).toContain('## 質問')
  })

  it('omits the image section without images', () => {
    const p = buildResearchPrompt('ワット・ラカンの創建年', { context: '一次資料: 1782年' })
    expect(p).not.toContain('画像の取り扱い')
    expect(p).toContain('既知の情報')
  })
})
