const { execSync } = require('child_process');

function runFullScript(query) {
    const script = `
        [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
        $ErrorActionPreference = 'Stop'
        try {
            $outlook = [System.Runtime.InteropServices.Marshal]::GetActiveObject("Outlook.Application")
        } catch {
            $outlook = New-Object -ComObject Outlook.Application
        }
        $namespace = $outlook.GetNamespace("MAPI")
        $inbox = $namespace.GetDefaultFolder(6) # olFolderInbox
        $filter = "@SQL=""urn:schemas:httpmail:subject"" like '%${query}%' OR ""urn:schemas:httpmail:textdescription"" like '%${query}%'"
        $items = $inbox.Items.Restrict($filter)
        $items.Sort("[ReceivedTime]", $true)
        $result = @()
        $count = 0
        foreach ($item in $items) {
            if ($count -ge 5) { break }
            $bodyPreview = ""
            if ($item.Body) {
                $bodyPreview = $item.Body.Substring(0, [math]::Min($item.Body.Length, 200))
            }
            $result += @{
                subject = $item.Subject
                sender = $item.SenderEmailAddress
                bodyPreview = $bodyPreview
                isRead = $item.UnRead
                id = $item.EntryID
            }
            $count++
        }
        $result | ConvertTo-Json -Depth 3 -Compress
`;
    const b64 = Buffer.from(script, 'utf16le').toString('base64');
    try {
        const out = execSync('powershell -NoProfile -EncodedCommand ' + b64, {encoding: 'utf8'});
        console.log(`Query: ${query} =>`, out.trim());
    } catch(e) {
        console.error(e.stdout || e.message);
    }
}

runFullScript('TL: Vv Thông báo kế hoạch tổ chức đào tạo nội bộ Công ty');
