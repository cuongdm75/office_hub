$ErrorActionPreference = 'Stop'
try {
    $outlook = [System.Runtime.InteropServices.Marshal]::GetActiveObject('Outlook.Application')
} catch {
    $outlook = New-Object -ComObject Outlook.Application
}
$namespace = $outlook.GetNamespace('MAPI')
$inbox = $namespace.GetDefaultFolder(6)
$query = 'TL: Vv Thông báo kế hoạch tổ chức đào tạo nội bộ Công ty'
$filter = "@SQL=""urn:schemas:httpmail:subject"" like '%" + $query + "%' OR ""urn:schemas:httpmail:textdescription"" like '%" + $query + "%'"
Write-Output "Filter: $filter"
try {
    $items = $inbox.Items.Restrict($filter)
    Write-Output "Result count: $($items.Count)"
} catch {
    Write-Output "Error: $_"
}
