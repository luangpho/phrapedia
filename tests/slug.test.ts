import { describe, it, expect } from 'vitest'
import { slugify, isValidSlug, shortId } from '../shared/utils/slug'

describe('slugify', () => {
  it('lower-cases and dashes Latin text', () => {
    expect(slugify('Luang Pho To')).toBe('luang-pho-to')
    expect(slugify('  Wat  Rakhang_Khositaram  ')).toBe('wat-rakhang-khositaram')
    expect(slugify('Somdet Phra Phutthachan (To)')).toBe('somdet-phra-phutthachan-to')
  })

  it('keeps Thai and Japanese characters', () => {
    expect(slugify('หลวงปู่ทวด')).toBe('หลวงปู่ทวด')
    expect(slugify('ワット ラカン')).toBe('ワット-ラカン')
    expect(slugify('ルアンポー・トー')).toBe('ルアンポー・トー')
  })

  it('normalises full-width characters (NFKC)', () => {
    expect(slugify('ＷＡＴ　１')).toBe('wat-1')
  })

  it('strips traversal and unsafe characters', () => {
    expect(slugify('../../etc/passwd')).toBe('etcpasswd')
    expect(slugify('a/b\\c?d#e')).toBe('abcde')
    expect(slugify('..')).toBe('')
    expect(slugify('.hidden')).toBe('hidden')
    expect(isValidSlug(slugify('../x'))).toBe(true)
  })

  it('returns empty string for nothing usable', () => {
    expect(slugify('')).toBe('')
    expect(slugify('   ')).toBe('')
    expect(slugify('///')).toBe('')
    expect(slugify(undefined as unknown as string)).toBe('')
  })
})

describe('isValidSlug', () => {
  it('accepts normal slugs', () => {
    expect(isValidSlug('somdet-to')).toBe(true)
    expect(isValidSlug('หลวงปู่ทวด')).toBe(true)
    expect(isValidSlug('a.b')).toBe(true)
  })

  it('rejects empty, traversal, separators, hidden and overlong', () => {
    expect(isValidSlug('')).toBe(false)
    expect(isValidSlug('.')).toBe(false)
    expect(isValidSlug('..')).toBe(false)
    expect(isValidSlug('.git')).toBe(false)
    expect(isValidSlug('a/b')).toBe(false)
    expect(isValidSlug('a\\b')).toBe(false)
    expect(isValidSlug('x'.repeat(121))).toBe(false)
    expect(isValidSlug('x'.repeat(120))).toBe(true)
  })
})

describe('shortId', () => {
  it('produces base36 ids of the requested length', () => {
    expect(shortId()).toMatch(/^[a-z0-9]{8}$/)
    expect(shortId(5)).toMatch(/^[a-z0-9]{5}$/)
  })
})
