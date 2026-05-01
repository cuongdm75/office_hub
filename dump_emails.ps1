$ErrorActionPreference = 'Stop'
try {
    $outlook = [System.Runtime.InteropServices.Marshal]::GetActiveObject('Outlook.Application')
} catch {
    $outlook = New-Object -ComObject Outlook.Application
}
$namespace = $outlook.GetNamespace('MAPI')
$inbox = $namespace.GetDefaultFolder(6)
$items = $inbox.Items
$items.Sort("[ReceivedTime]", $true)
$count = 0
foreach ($item in $items) {
    if ($count -ge 10) { break }
    Write-Output "Subject: $($item.Subject)"
    $count++
}
