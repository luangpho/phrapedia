import { z } from 'zod'
import type { AppSettings } from '../../../shared/types/settings'

const nonEmpty = z.string().trim().min(1).max(1000)
const gitName = z.string().trim().max(200).refine(isSafeGitName, 'git の名前として使えない文字が含まれています')

/** Shape accepted by PUT /api/settings (every field optional; secrets are never accepted here). */
const SettingsPatch = z.object({
  sourcesDir: nonEmpty.optional(),
  dataDir: nonEmpty.optional(),
  ai: z.object({
    model: nonEmpty.max(100).optional(),
    maxTokens: z.number().int().min(256).max(128_000).optional(),
    extraInstructions: z.string().max(20_000).optional(),
    command: nonEmpty.optional(),
    args: z.array(z.string().max(1000)).max(50).optional(),
    timeoutMs: z.number().int().min(1000).max(3_600_000).optional(),
  }).strict().optional(),
  gemini: z.object({
    engine: z.enum(['antigravity', 'gemini-cli']).optional(),
    command: nonEmpty.optional(),
    args: z.array(z.string().max(1000)).max(50).optional(),
    model: z.string().trim().max(100).optional(),
    timeoutMs: z.number().int().min(1000).max(3_600_000).optional(),
    cacheHours: z.number().min(0).max(24 * 365).optional(),
  }).strict().optional(),
  git: z.object({
    remote: gitName.optional(),
    branch: gitName.optional(),
    authorName: z.string().trim().max(200).optional(),
    authorEmail: z.string().trim().max(200).optional(),
    autoCommit: z.boolean().optional(),
    autoPush: z.boolean().optional(),
  }).strict().optional(),
}).strict()

export default defineEventHandler(async (event) => {
  const body = await readBody<unknown>(event)
  const parsed = SettingsPatch.safeParse(body)
  if (!parsed.success) {
    const issues = parsed.error.issues.map(i => `${i.path.join('.') || '(root)'}: ${i.message}`).join('; ')
    throw createError({ statusCode: 400, statusMessage: 'Invalid settings', message: `設定の値が不正です: ${issues}` })
  }
  // Never allow secrets through this endpoint; they stay in env / runtimeConfig.
  return updateSettings(parsed.data as Partial<AppSettings>)
})
