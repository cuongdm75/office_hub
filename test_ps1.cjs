const { execSync } = require('child_process');

function runPS(query) {
    const script = `
$ErrorActionPreference = 'Stop'
try { $o = [System.Runtime.InteropServices.Marshal]::GetActiveObject('Outlook.Application') } 
catch { $o = New-Object -ComObject Outlook.Application }
$ns = $o.GetNamespace('MAPI')
$inbox = $ns.GetDefaultFolder(6)
$f = "@SQL=\"\"urn:schemas:httpmail:subject\"\" like '%${query}%' OR \"\"urn:schemas:httpmail:textdescription\"\" like '%${query}%'"
try { 
  $i = $inbox.Items.Restrict($f)
  $i.Sort("[ReceivedTime]", $true)
  Write-Output "Count: $($i.Count)"
} catch { 
  Write-Output "Error: $($_.Exception.Message)"
}
`;
    const b64 = Buffer.from(script, 'utf16le').toString('base64');
    try {
        const out = execSync('powershell -NoProfile -EncodedCommand ' + b64, {encoding: 'utf8'});
        console.log(`Query: ${query} =>`, out.trim());
    } catch(e) {
        console.error(e.stdout || e.message);
    }
}

runPS('Thông báo');
