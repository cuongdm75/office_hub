import io
with open('C:/Users/admin/.gemini/antigravity/brain/4c81942f-6115-4a16-b6ab-19ce12cdc1d3/task.md', 'r', encoding='utf-8') as f:
    text = f.read()

text = text.replace('- [ ] Phase 3:', '- [x] Phase 3:')
text = text.replace('- [ ] Refactor Analyst Agent.', '- [x] Refactor Analyst Agent.')
text = text.replace('- [ ] Refactor Office Master Agent.', '- [x] Refactor Office Master Agent.')
text = text.replace('- [ ] Refactor Web Researcher Agent.', '- [x] Refactor Web Researcher Agent.')
text = text.replace('- [ ] Cung cấp MCP Client cho Agent để gọi chéo (Agent-to-Agent).', '- [x] Cung cấp MCP Client cho Agent để gọi chéo (Agent-to-Agent).')

with open('C:/Users/admin/.gemini/antigravity/brain/4c81942f-6115-4a16-b6ab-19ce12cdc1d3/task.md', 'w', encoding='utf-8') as f:
    f.write(text)
print("Task updated")
