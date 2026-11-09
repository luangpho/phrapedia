import type { SettingsView } from '../../../shared/types/settings'
import { probeClaude } from '../../utils/agent/claude-cli'

export default defineEventHandler(async (): Promise<SettingsView> => {
  const s = await getSettings()
  const [gemini, claude] = await Promise.all([probeGemini(), probeClaude(s.ai.command)])
  return {
    ...s,
    resolved: { sourcesDir: resolveFromRoot(s.sourcesDir), dataDir: resolveFromRoot(s.dataDir), projectRoot: projectRoot() },
    capabilities: { claudeFound: claude.found, claudeVersion: claude.version, geminiFound: gemini.found, geminiVersion: gemini.version, geminiEngine: ENGINE_LABELS[gemini.engine] },
  }
})
