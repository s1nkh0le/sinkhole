import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      globals: globals.browser,
    },
    rules: {
      // Require strict equality except against null/undefined, where the loose
      // form is a deliberate "nullish" check used throughout the codebase.
      eqeqeq: ['error', 'always', { null: 'ignore' }],
      // Keep stray debug logging out of the build; warn/error stay allowed.
      'no-console': ['warn', { allow: ['warn', 'error'] }],
    },
  },
])
