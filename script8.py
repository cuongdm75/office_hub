import io
with open('src-tauri/src/orchestrator/memory.rs', 'r', encoding='utf-8') as f:
    text = f.read()
text = text.replace('use tracing::{info, warn, debug};', 'use tracing::{info, debug};')
with open('src-tauri/src/orchestrator/memory.rs', 'w', encoding='utf-8') as f:
    f.write(text)

with open('src-tauri/src/agents/office_master/com_word.rs', 'r', encoding='utf-8') as f:
    text = f.read()
text = text.replace('use crate::agents::com_utils::dispatch::{var_bool, var_bstr, var_i4, var_optional, ComObject};', 'use crate::agents::com_utils::dispatch::{var_bool, var_bstr, var_i4, ComObject};')
with open('src-tauri/src/agents/office_master/com_word.rs', 'w', encoding='utf-8') as f:
    f.write(text)

with open('src-tauri/src/agents/web_researcher/uia.rs', 'r', encoding='utf-8') as f:
    text = f.read()
text = text.replace('    IUIAutomationCacheRequest, UIA_NamePropertyId,', '    UIA_NamePropertyId,')
with open('src-tauri/src/agents/web_researcher/uia.rs', 'w', encoding='utf-8') as f:
    f.write(text)

with open('src-tauri/src/agents/com_utils.rs', 'r', encoding='utf-8') as f:
    text = f.read()
text = text.replace('    use tracing::{warn, debug};', '    use tracing::warn;')
with open('src-tauri/src/agents/com_utils.rs', 'w', encoding='utf-8') as f:
    f.write(text)
