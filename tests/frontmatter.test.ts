import { describe, it, expect } from 'vitest'
import { parseFrontmatter, stringifyFrontmatter } from '../server/utils/frontmatter'

describe('frontmatter', () => {
  it('round-trips Thai / Japanese strings and arrays', () => {
    const data = {
      slug: 'somdet-to',
      kind: 'monks',
      name: 'ソムデット・トー',
      name_th: 'สมเด็จพระพุฒาจารย์ (โต พฺรหฺมรํสี)',
      aliases: ['ルアンポー・トー', 'หลวงพ่อโต'],
      tags: [],
    }
    const body = '# ソムデット・トー\n\n## 概要\nワット・ラカンの住職。\n'
    const raw = stringifyFrontmatter(data, body)
    expect(raw.startsWith('---\n')).toBe(true)
    expect(raw).toContain('สมเด็จพระพุฒาจารย์') // unicode is not escaped
    const parsed = parseFrontmatter(raw)
    expect(parsed.data).toEqual(data)
    // stringify puts one blank line after the closing ---, which the parser keeps as part of the body
    expect(parsed.body.replace(/^\n/, '')).toBe(body)
    // re-serialising is stable (no blank lines accumulate)
    expect(stringifyFrontmatter(parsed.data, parsed.body)).toBe(raw)
  })

  it('quotes values that contain colons and keeps them intact', () => {
    const data = { name: 'Wat: Rakhang', note: '時刻 10:30 / key: value', url: 'https://example.com/a?b=c' }
    const parsed = parseFrontmatter(stringifyFrontmatter(data, 'x'))
    expect(parsed.data).toEqual(data)
  })

  it('keeps numeric-looking years as strings when quoted', () => {
    const parsed = parseFrontmatter(stringifyFrontmatter({ born: '1788' }, ''))
    expect(parsed.data.born).toBe('1788')
  })

  it('turns unquoted YAML dates into YYYY-MM-DD strings', () => {
    const parsed = parseFrontmatter('---\nborn: 1788-01-01\ncreated_at: 2026-11-05\n---\nbody\n')
    expect(parsed.data.born).toBe('1788-01-01')
    expect(parsed.data.created_at).toBe('2026-11-05')
    expect(typeof parsed.data.created_at).toBe('string')
  })

  it('drops undefined values when stringifying', () => {
    const raw = stringifyFrontmatter({ a: 1, b: undefined }, 'x')
    expect(raw).not.toContain('b:')
  })

  it('returns empty data when there is no frontmatter', () => {
    const parsed = parseFrontmatter('# タイトル\n本文')
    expect(parsed.data).toEqual({})
    expect(parsed.body).toBe('# タイトル\n本文')
  })

  it('handles a BOM and CRLF line endings', () => {
    const parsed = parseFrontmatter('﻿---\r\nname: テスト\r\n---\r\nbody')
    expect(parsed.data).toEqual({ name: 'テスト' })
    expect(parsed.body).toBe('body')
  })

  it('throws a 400 on invalid YAML', () => {
    expect(() => parseFrontmatter('---\nname: [unclosed\n---\nbody')).toThrowError(/YAML frontmatter could not be parsed/)
    try {
      parseFrontmatter('---\nname: [unclosed\n---\nbody')
    } catch (err) {
      expect((err as { statusCode?: number }).statusCode).toBe(400)
    }
  })

  it('throws when frontmatter is not a mapping', () => {
    expect(() => parseFrontmatter('---\n- a\n- b\n---\nbody')).toThrowError(/mapping/)
  })
})
