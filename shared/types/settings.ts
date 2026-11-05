export type ChatProvider = 'anthropic' | 'gemini-cli'

export interface AppSettings {
  /** Directory holding primary sources (markdown/images). Absolute, or relative to the project root. */
  sourcesDir: string
  /** Directory holding records (data/<kind>/<slug>.md). Absolute, or relative to the project root. */
  dataDir: string
  ai: {
    provider: ChatProvider
    anthropicModel: string
    maxTokens: number
    /** Extra instructions appended to the system prompt (user-editable) */
    extraInstructions: string
  }
  gemini: {
    /** Executable name or absolute path of the Gemini CLI */
    command: string
    /** Extra CLI arguments inserted before the prompt (e.g. ["-m", "gemini-2.5-pro"]) */
    args: string[]
    /** Model passed with -m (e.g. "flash", "pro", "gemini-2.5-pro"). Empty = CLI default ("auto"). */
    model: string
    /**
     * Gemini CLI approval mode. In headless mode google_web_search is always allowed; web_fetch is denied under
     * "default" and allowed under "auto_edit" / "yolo". "yolo" also allows shell/file tools, so prefer "auto_edit".
     */
    approvalMode: 'default' | 'auto_edit' | 'yolo'
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
    provider: 'anthropic',
    anthropicModel: 'claude-sonnet-5-5',
    maxTokens: 8192,
    extraInstructions: '',
  },
  gemini: {
    command: 'gemini',
    args: [],
    model: '',
    approvalMode: 'auto_edit',
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
  }
}
