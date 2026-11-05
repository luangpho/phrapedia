export default defineAppConfig({
  ui: {
    colors: {
      primary: 'amber',
      neutral: 'zinc',
    },
    button: {
      slots: { base: 'rounded-md' },
    },
    card: {
      slots: { root: 'rounded-md shadow-sm' },
    },
  },
})
