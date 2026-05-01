import io
with open('src-tauri/src/orchestrator/mod.rs', 'r', encoding='utf-8') as f:
    text = f.read()

text = text.replace('if res.type_ == "text" {', 'if res.content_type == "text" {')

with open('src-tauri/src/orchestrator/mod.rs', 'w', encoding='utf-8') as f:
    f.write(text)
