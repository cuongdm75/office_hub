$ErrorActionPreference = 'Stop'
try {
    $outlook = [System.Runtime.InteropServices.Marshal]::GetActiveObject('Outlook.Application')
} catch {
    $outlook = New-Object -ComObject Outlook.Application
}
$namespace = $outlook.GetNamespace('MAPI')
$inbox = $namespace.GetDefaultFolder(6)
$query = "Thông báo"
$filter = "@SQL=`"urn:schemas:httpmail:subject`" like '%$query%'"
Write-Output "Filter: $filter"
$items = $inbox.Items.Restrict($filter)
Write-Output $items.Count
