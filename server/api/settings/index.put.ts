import type { AppSettings } from '../../../shared/types/settings'

export default defineEventHandler(async (event) => {
  const patch = await readBody<Partial<AppSettings>>(event)
  if (!patch || typeof patch !== 'object') throw createError({ statusCode: 400, message: 'settings object required' })
  // Never allow secrets through this endpoint; they stay in env / runtimeConfig.
  const next = await updateSettings(patch)
  return next
})
