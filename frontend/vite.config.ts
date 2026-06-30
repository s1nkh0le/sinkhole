import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
// base '/ui/' so the app is served under /ui by the controller (the API stays at
// the origin root, where the SPA's client routes like /services would otherwise
// collide with the API). The API client uses window.location.origin, not this base.
export default defineConfig({
  base: '/ui/',
  plugins: [react()],
})
