import io
with open('src-tauri/src/mcp/broker.rs', 'r', encoding='utf-8') as f:
    text = f.read()

text = text.replace('use anyhow::{Result, anyhow};', 'use anyhow::Result;')

with open('src-tauri/src/mcp/broker.rs', 'w', encoding='utf-8') as f:
    f.write(text)
