import { beforeEach, describe, expect, it, vi } from 'vitest'
import { DEFAULT_SETTINGS } from '../shared/types/settings'
import type { SettingsView } from '../shared/types/settings'

const probeClaudeMock = vi.hoisted(() => vi.fn())
vi.mock('../server/utils/agent/claude-cli', () => ({ probeClaude: probeClaudeMock }))

import { migrateStoredSettings } from '../server/utils/settings'

const g = globalThis as Record<string, unknown>

describe('default AI settings', () => {
  it('runs Claude through the CLI with the previous model default', () => {
    expect(DEFAULT_SETTINGS.ai).toEqual({
      model: 'claude-sonnet-5-5',
      maxTokens: 8192,
      extraInstructions: '',
      command: 'claude',
      args: [],
      timeoutMs: 600_000,
    })
    expect(DEFAULT_SETTINGS.ai).not.toHaveProperty('anthropicModel')
  })
})

describe('migrateStoredSettings', () => {
  it('moves ai.anthropicModel to ai.model and drops the old key', () => {
    const migrated = migrateStoredSettings({ ai: { anthropicModel: 'claude-opus-5-5', maxTokens: 4000 } } as never)
    expect(migrated?.ai).toEqual({ model: 'claude-opus-5-5', maxTokens: 4000 })
  })
  it('keeps an explicit ai.model and just drops the old key', () => {
    const migrated = migrateStoredSettings({ ai: { anthropicModel: 'old', model: 'new' } } as never)
    expect(migrated?.ai).toEqual({ model: 'new' })
  })
  it('leaves settings without the old key untouched', () => {
    const stored = { sourcesDir: 'x', ai: { model: 'm' } } as never
    expect(migrateStoredSettings(stored)).toBe(stored)
    expect(migrateStoredSettings(undefined)).toBeUndefined()
    expect(migrateStoredSettings(null)).toBeUndefined()
  })
})

describe('GET /api/settings capabilities', () => {
  beforeEach(() => {
    probeClaudeMock.mockReset()
    g.defineEventHandler = (fn: unknown) => fn
    g.getSettings = async () => DEFAULT_SETTINGS
    g.probeGemini = async () => ({ found: true, version: '1.4.8', command: 'agy', engine: 'antigravity' })
    g.ENGINE_LABELS = { 'antigravity': 'Antigravity CLI (agy)', 'gemini-cli': 'Gemini CLI (gemini)' }
    g.resolveFromRoot = (p: string) => `/root/${p}`
    g.projectRoot = () => '/root'
  })

  const call = async () => (await import('../server/api/settings/index.get')).default({} as never) as unknown as Promise<SettingsView>

  it('reports claudeFound / claudeVersion instead of an API key flag', async () => {
    probeClaudeMock.mockResolvedValue({ found: true, command: 'claude', version: '2.1.317 (Claude Code)' })
    const view = await call()
    expect(probeClaudeMock).toHaveBeenCalledWith('claude')
    expect(view.capabilities).toEqual({
      claudeFound: true,
      claudeVersion: '2.1.317 (Claude Code)',
      geminiFound: true,
      geminiVersion: '1.4.8',
      geminiEngine: 'Antigravity CLI (agy)',
    })
    expect(view.capabilities).not.toHaveProperty('anthropicConfigured')
  })

  it('reports a missing CLI', async () => {
    probeClaudeMock.mockResolvedValue({ found: false, command: 'claude', error: 'ENOENT' })
    const view = await call()
    expect(view.capabilities.claudeFound).toBe(false)
    expect(view.capabilities.claudeVersion).toBeUndefined()
  })
})
