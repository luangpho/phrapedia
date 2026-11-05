export interface AppSettings {
  /** Directory holding primary sources (markdown/images). Absolute, or relative to the project root. */
  sourcesDir: string
  /** Directory holding records (data/<kind>/<slug>.md). Absolute, or relative to the project root. */
  dataDir: string
  /** Chat assistant — always Anthropic Claude. Gemini is used only by the web_research tool. */
  ai: {
    anthropicModel: string
    maxTokens: number
    /** Extra instructions appended to the system prompt (user-editable) */
    extraInstructions: string
  }
  gemini: {
    /**
     * Which Google CLI performs the web research.
     * - antigravity: Antigravity CLI (`agy`) — replaced Gemini CLI for unpaid / Google One users in June 2026
     * - gemini-cli: legacy `@google/gemini-cli` (`gemini`), still usable with paid tiers / API keys
     */
    engine: 'antigravity' | 'gemini-cli'
    /** Executable name or absolute path of the CLI (`agy` or `gemini`) */
    command: string
    /** Extra CLI arguments inserted before the prompt (e.g. ["-m", "gemini-2.5-pro"]) */
    args: string[]
    /** Model passed with -m (e.g. "flash", "pro", "gemini-2.5-pro"). Empty = CLI default ("auto"). */
    model: string
    timeoutMs: number
    /** Cache web research results for this many hours (0 = no cache) */
    cacheHours: number
  }
  git: {
    remote: string
    branch: string
    authorName: string
    authorEmail: string
    /** Commit automatically after each save */
    autoCommit: boolean
    /** Push automatically after each auto-commit */
    autoPush: boolean
  }
}

export const DEFAULT_SETTINGS: AppSettings = {
  sourcesDir: 'sources',
  dataDir: 'data',
  ai: {
    anthropicModel: 'claude-sonnet-5-5',
    maxTokens: 8192,
    extraInstructions: '',
  },
  gemini: {
    engine: 'antigravity',
    command: 'agy',
    args: [],
    model: '',
    timeoutMs: 180_000,
    cacheHours: 24,
  },
  git: {
    remote: 'origin',
    branch: '',
    authorName: '',
    authorEmail: '',
    autoCommit: false,
    autoPush: false,
  },
}

/** Settings as returned to the client (secrets never included; adds derived info). */
export interface SettingsView extends AppSettings {
  resolved: {
    sourcesDir: string
    dataDir: string
    projectRoot: string
  }
  capabilities: {
    anthropicConfigured: boolean
    geminiFound: boolean
    geminiVersion?: string
    /** Human label of the configured research CLI, e.g. "Antigravity CLI (agy)" */
    geminiEngine: string
  }
}
