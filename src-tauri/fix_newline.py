import re
file_path = 'e:/Office hub/src-tauri/src/orchestrator/mod.rs'
with open(file_path, 'r', encoding='utf-8') as f:
    content = f.read()

content = content.replace("text_buf.push('\\n');", "text_buf.push('\\\\n');")
content = content.replace("text_buf.push('" + chr(10) + "');", "text_buf.push('\\\\n');")

with open(file_path, 'w', encoding='utf-8') as f:
    f.write(content)
