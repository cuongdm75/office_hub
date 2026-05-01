import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import fs from 'fs'
import path from 'path'
import os from 'os'

const certsDir = path.join(os.homedir(), '.office-addin-dev-certs')
const keyPath = path.join(certsDir, 'localhost.key')
const certPath = path.join(certsDir, 'localhost.crt')

let httpsConfig = undefined
if (fs.existsSync(keyPath) && fs.existsSync(certPath)) {
  httpsConfig = {
    key: fs.readFileSync(keyPath),
    cert: fs.readFileSync(certPath)
  }
}

export default defineConfig({
  plugins: [react()],
  server: {
    host: '127.0.0.1',
    port: 3000,
    https: httpsConfig
  }
})
