// https://nuxt.com/docs/api/configuration/nuxt-config
// Desktop (Tauri) frontend: a static SPA only (no server runtime). `nuxt generate` -> .output-frontend/public
// (= tauri.conf.json build.frontendDist). All data goes through Tauri commands (app/utils/api.ts).
export default defineNuxtConfig({
  compatibilityDate: '2025-07-15',
  devtools: { enabled: true },
  modules: ['@nuxt/ui'],
  css: ['~/assets/css/main.css'],
  ssr: false,

  app: {
    head: {
      htmlAttrs: { lang: 'ja' },
      title: 'Phrapedia',
      meta: [{ name: 'description', content: 'タイの高僧・寺院・プラクルアン情報データベース' }],
    },
  },

  runtimeConfig: {
    public: {
      appName: 'Phrapedia',
    },
  },

  icon: {
    // No icon server and no network: every icon used by the app is bundled into the client.
    provider: 'none',
    serverBundle: false,
    fallbackToApi: false,
    clientBundle: { scan: true, sizeLimitKb: 512 },
  },

  nitro: {
    output: { dir: '.output-frontend' },
  },

  devServer: { port: 1420 },

  typescript: {
    strict: true,
    typeCheck: false,
  },
})
