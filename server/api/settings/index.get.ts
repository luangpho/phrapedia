import type { SettingsView } from '../../../shared/types/settings'

export default defineEventHandler(async (): Promise<SettingsView> => {
  const s = await getSettings()
  const cfg = useRuntimeConfig()
  const gemini = await probeGemini()
  return {
    ...s,
    resolved: { sourcesDir: resolveFromRoot(s.sourcesDir), dataDir: resolveFromRoot(s.dataDir), projectRoot: projectRoot() },
    capabilities: { anthropicConfigured: Boolean(cfg.anthropicApiKey || process.env.ANTHROPIC_API_KEY), geminiFound: gemini.found, geminiVersion: gemini.version },
  }
})
