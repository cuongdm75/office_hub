import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import fs from 'fs'
import path from 'path'
import os from 'os'

const certsDir = path.join(os.homedir(), '.office-addin-dev-certs')

export default defineConfig({
  plugins: [react()],
  server: {
    host: '127.0.0.1',
    port: 3000,
    https: {
      key: fs.readFileSync(path.join(certsDir, 'localhost.key')),
      cert: fs.readFileSync(path.join(certsDir, 'localhost.crt'))
    }
  }
})
