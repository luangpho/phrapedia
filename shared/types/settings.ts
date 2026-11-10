export interface AppSettings {
  /** Directory holding primary sources (markdown/images). Absolute, or relative to the project root. */
  sourcesDir: string
  /** Directory holding records (data/<kind>/<slug>.md). Absolute, or relative to the project root. */
  dataDir: string
  /** Chat assistant — always Claude, run through the Claude Code CLI (`claude -p`). Gemini is used only by the web_research tool. */
  ai: {
    model: string
    maxTokens: number
    /** Extra instructions appended to the system prompt (user-editable) */
    extraInstructions: string
    /** Executable name or absolute path of the Claude Code CLI */
    command: string
    /** Extra CLI arguments inserted before the generated ones (e.g. ["--effort", "low"]) */
    args: string[]
    /** Kill the CLI when one chat turn takes longer than this */
    timeoutMs: number
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
    model: 'claude-sonnet-5-5',
    maxTokens: 8192,
    extraInstructions: '',
    command: 'claude',
    args: [],
    timeoutMs: 600_000,
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
    /** Dataset root (read source; holds data/<kind>/ and sources/) */
    datasetRoot: string
    /** Export root (write target; the git sync scope) */
    exportRoot: string
  }
  capabilities: {
    /** `claude --version` ran successfully */
    claudeFound: boolean
    claudeVersion?: string
    geminiFound: boolean
    geminiVersion?: string
    /** Human label of the configured research CLI, e.g. "Antigravity CLI (agy)" */
    geminiEngine: string
  }
}
