import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtemp, rm, readFile, writeFile, mkdir } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTestDataDir } from './setup'
import {
  entityTemplate, writeEntity, getEntity, listEntities, deriveSlug, deleteEntity, entityExists, assertKind, assertSlug,
} from '../server/utils/entities'
import { parseFrontmatter } from '../server/utils/frontmatter'

describe('entityTemplate', () => {
  it('builds a monk template with the expected keys and Japanese headings', () => {
    const raw = entityTemplate('monks', 'ルアンポー・トー')
    const { data, body } = parseFrontmatter(raw)
    expect(Object.keys(data)).toEqual(['slug', 'kind', 'name', 'name_th', 'name_en', 'status', 'sources', 'tags', 'temple', 'province', 'born', 'died'])
    expect(data.kind).toBe('monks')
    expect(data.name).toBe('ルアンポー・トー')
    expect(data.status).toBe('draft')
    expect(data.sources).toEqual([])
    expect(body).toContain('# ルアンポー・トー')
    for (const h of ['## 概要', '## 生涯', '## 所属寺院', '## 代表的なプラクルアン', '## 出典']) expect(body).toContain(h)
  })

  it('builds a temple template', () => {
    const { data, body } = parseFrontmatter(entityTemplate('temples'))
    expect(data.kind).toBe('temples')
    expect(data).toHaveProperty('founded')
    expect(data).not.toHaveProperty('temple')
    expect(body).toContain('# （寺院名）')
    for (const h of ['## 概要', '## 歴史', '## 関連する高僧', '## 所在地', '## 出典']) expect(body).toContain(h)
  })

  it('builds an amulet template', () => {
    const { data, body } = parseFrontmatter(entityTemplate('amulets', 'プラ・ソムデット'))
    expect(data.kind).toBe('amulets')
    expect(body).toContain('# プラ・ソムデット')
  })
})

describe('assertKind / assertSlug', () => {
  it('validates kinds', () => {
    expect(assertKind('monks')).toBe('monks')
    expect(() => assertKind('people')).toThrowError(/Unknown entity kind/)
    expect(() => assertKind(undefined)).toThrow()
  })
  it('decodes and validates slugs', () => {
    expect(assertSlug(encodeURIComponent('หลวงปู่ทวด'))).toBe('หลวงปู่ทวด')
    expect(() => assertSlug('..')).toThrowError(/Invalid slug/)
    expect(() => assertSlug(encodeURIComponent('../x'))).toThrowError(/Invalid slug/)
  })
})

describe('entity storage', () => {
  let dir: string

  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), 'phrapedia-test-'))
    setTestDataDir(dir)
  })
  afterEach(async () => {
    await rm(dir, { recursive: true, force: true })
  })

  const sample = (extra = '') => `---
name: ソムデット・トー
name_th: สมเด็จพระพุฒาจารย์ (โต พฺรหฺมรํสี)
name_en: Somdet To
temple: wat-rakhang
born: "1788"
custom_key: keep me
${extra}---

# ソムデット・トー

## 概要
本文
`

  it('writes, reads and normalises a record', async () => {
    const rec = await writeEntity('monks', 'somdet-to', sample('slug: wrong\nkind: temples\nstatus: bogus\n'))
    expect(rec.slug).toBe('somdet-to')
    expect(rec.kind).toBe('monks')
    expect(rec.path).toBe('data/monks/somdet-to.md')
    expect(rec.frontmatter.slug).toBe('somdet-to')
    expect(rec.frontmatter.kind).toBe('monks')
    expect(rec.status).toBe('draft')
    expect(rec.frontmatter.custom_key).toBe('keep me')
    expect(rec.frontmatter.born).toBe('1788')
    expect(typeof rec.frontmatter.created_at).toBe('string')
    expect(rec.frontmatter.updated_at).toBe(rec.frontmatter.created_at)

    const onDisk = await readFile(join(dir, 'monks', 'somdet-to.md'), 'utf8')
    expect(onDisk).toContain('slug: somdet-to')
    expect(onDisk).toContain('kind: monks')
    expect(onDisk).toContain('สมเด็จพระพุฒาจารย์')

    const got = await getEntity('monks', 'somdet-to')
    expect(got?.name).toBe('ソムデット・トー')
    expect(got?.name_th).toBe('สมเด็จพระพุฒาจารย์ (โต พฺรหฺมรํสี)')
    expect(got?.body).toContain('## 概要')
  })

  it('falls back to the H1 title, then the slug, for the name', async () => {
    const a = await writeEntity('temples', 'wat-a', '# ワット・エー\n\n本文\n')
    expect(a.name).toBe('ワット・エー')
    const b = await writeEntity('temples', 'wat-b', 'no title here\n')
    expect(b.name).toBe('wat-b')
  })

  it('preserves created_at on update and bumps updated_at', async () => {
    const first = await writeEntity('monks', 'somdet-to', sample())
    const created = first.frontmatter.created_at
    await new Promise(r => setTimeout(r, 10))
    // raw without created_at: must be taken from the previous version
    const second = await writeEntity('monks', 'somdet-to', sample().replace('## 概要', '## 概要（改訂）'), { mustExist: true })
    expect(second.frontmatter.created_at).toBe(created)
    expect(second.frontmatter.updated_at).not.toBe(created)
    expect(second.body).toContain('（改訂）')
  })

  it('keeps an explicit created_at from the input', async () => {
    const rec = await writeEntity('monks', 'x', sample('created_at: 2020-01-01T00:00:00.000Z\n'))
    expect(rec.frontmatter.created_at).toBe('2020-01-01T00:00:00.000Z')
  })

  it('honours mustNotExist / mustExist', async () => {
    await writeEntity('monks', 'somdet-to', sample())
    await expect(writeEntity('monks', 'somdet-to', sample(), { mustNotExist: true })).rejects.toMatchObject({ statusCode: 409 })
    await expect(writeEntity('monks', 'nope', sample(), { mustExist: true })).rejects.toMatchObject({ statusCode: 404 })
  })

  it('lists records sorted by name, skipping README and dotfiles, tolerating broken files', async () => {
    await writeEntity('monks', 'b', '---\nname: ルアンプー・トゥアット\n---\n')
    await writeEntity('monks', 'a', '---\nname: ソムデット・トー\nstatus: verified\n---\n')
    await mkdir(join(dir, 'monks'), { recursive: true })
    await writeFile(join(dir, 'monks', 'README.md'), '# readme')
    await writeFile(join(dir, 'monks', '.hidden.md'), '# hidden')
    await writeFile(join(dir, 'monks', 'broken.md'), '---\nname: [oops\n---\n')
    const list = await listEntities('monks')
    expect(list.map(e => e.slug).sort()).toEqual(['a', 'b', 'broken'])
    const broken = list.find(e => e.slug === 'broken')!
    expect(broken.name).toBe('broken')
    expect(broken.status).toBe('draft')
    expect(list.find(e => e.slug === 'a')?.status).toBe('verified')
    const names = list.filter(e => e.slug !== 'broken').map(e => e.name)
    expect(names).toEqual([...names].sort((x, y) => x.localeCompare(y, 'ja')))
  })

  it('returns an empty list for an empty kind and null for a missing record', async () => {
    expect(await listEntities('temples')).toEqual([])
    expect(await getEntity('temples', 'missing')).toBeNull()
  })

  it('derives slugs: slug > name_en > name > name_th > random, unique with -2, -3', async () => {
    expect(await deriveSlug('monks', { slug: 'Explicit Slug', name_en: 'Other' })).toBe('explicit-slug')
    expect(await deriveSlug('monks', { name_en: 'Luang Pu Thuat', name: 'ルアンプー・トゥアット' })).toBe('luang-pu-thuat')
    expect(await deriveSlug('monks', { name: 'ルアンプー・トゥアット' })).toBe('ルアンプー・トゥアット')
    expect(await deriveSlug('monks', { name: '///', name_th: 'หลวงปู่ทวด' })).toBe('หลวงปู่ทวด')
    expect(await deriveSlug('monks', {})).toMatch(/^monk-[a-z0-9]{6}$/)
    expect(await deriveSlug('temples', { name: '???' })).toMatch(/^temple-[a-z0-9]{6}$/)

    await writeEntity('monks', 'luang-pu-thuat', '# A\n')
    expect(await deriveSlug('monks', { name_en: 'Luang Pu Thuat' })).toBe('luang-pu-thuat-2')
    await writeEntity('monks', 'luang-pu-thuat-2', '# B\n')
    expect(await deriveSlug('monks', { name_en: 'Luang Pu Thuat' })).toBe('luang-pu-thuat-3')
  })

  it('deletes records and 404s on missing ones', async () => {
    await writeEntity('temples', 'wat-rakhang', '# ワット・ラカン\n')
    expect(await entityExists('temples', 'wat-rakhang')).toBe(true)
    await deleteEntity('temples', 'wat-rakhang')
    expect(await entityExists('temples', 'wat-rakhang')).toBe(false)
    await expect(deleteEntity('temples', 'wat-rakhang')).rejects.toMatchObject({ statusCode: 404 })
  })
})

describe('sample dataset', () => {
  it('parses every record under data/ with a name and matching slug/kind', async () => {
    const root = join(import.meta.dirname, '..', 'data')
    setTestDataDir(root)
    const expected: Record<string, string[]> = { monks: ['luang-pu-thuat', 'somdet-to'], temples: ['wat-chang-hai', 'wat-rakhang'] }
    for (const [kind, slugs] of Object.entries(expected)) {
      const list = await listEntities(kind as 'monks' | 'temples')
      for (const slug of slugs) {
        const rec = await getEntity(kind as 'monks' | 'temples', slug)
        expect(rec, `${kind}/${slug}`).not.toBeNull()
        expect(rec!.frontmatter.slug).toBe(slug)
        expect(rec!.frontmatter.kind).toBe(kind)
        expect(rec!.name_th).toBeTruthy()
        expect(rec!.status).toBe('draft')
        expect(list.some(e => e.slug === slug)).toBe(true)
      }
    }
    const to = await getEntity('monks', 'somdet-to')
    expect(to!.frontmatter.temple).toBe('wat-rakhang')
    expect(to!.frontmatter.born).toBe('1788')
    expect(to!.body).toContain('(/temples/wat-rakhang)')
  })
})
