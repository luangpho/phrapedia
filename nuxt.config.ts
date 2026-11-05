// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  devtools: { enabled: true },
  modules: ['@nuxt/ui', '@nuxtjs/mcp-toolkit'],
  css: ['~/assets/css/main.css'],
  ssr: true,

  app: {
    head: {
      htmlAttrs: { lang: 'ja' },
      title: 'Phrapedia',
      meta: [{ name: 'description', content: 'タイの高僧・寺院・プラクルアン情報データベース' }],
    },
  },

  runtimeConfig: {
    // Server-only. Override with NUXT_* environment variables (see .env.example).
    anthropicApiKey: '',
    anthropicModel: 'claude-sonnet-5-5',
    dataDir: 'data',
    sourcesDir: 'sources',
    stateDir: '.phrapedia',
    geminiCommand: 'gemini',
    public: {
      appName: 'Phrapedia',
    },
  },

  icon: {
    // Bundle the whole lucide collection server-side so dynamically named icons never hit the Iconify API
    // (the app must work offline / behind restrictive proxies).
    serverBundle: { collections: ['lucide'] },
    clientBundle: { scan: true, sizeLimitKb: 512 },
  },

  mcp: {
    name: 'Phrapedia',
    version: '0.1.0',
    description: 'Knowledge base of Thai venerable monks, temples and amulets (Phra Khrueang).',
    instructions: [
      'Primary sources (files under the configured sources directory) are authoritative.',
      'Use search_sources / read_source first; use web_research (Gemini CLI) only for gaps, and mark such facts as unverified.',
      'Records are markdown files with YAML frontmatter under data/<kind>/<slug>.md; use get_entity / save_entity to read and write them.',
    ].join(' '),
    route: '/mcp',
  },

  nitro: {
    experimental: { asyncContext: true },
    // PGlite ships a WASM bundle; keep it external so Nitro does not try to inline it.
    externals: { external: ['@electric-sql/pglite'] },
  },

  vite: {
    optimizeDeps: { exclude: ['@electric-sql/pglite'] },
  },

  typescript: {
    strict: true,
    typeCheck: false,
  },
})
