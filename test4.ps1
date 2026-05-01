$ErrorActionPreference = 'Stop'
try { $o = [System.Runtime.InteropServices.Marshal]::GetActiveObject('Outlook.Application') } catch { $o = New-Object -ComObject Outlook.Application }
$ns = $o.GetNamespace('MAPI')
$inbox = $ns.GetDefaultFolder(6)
$query = 'TL: Vv Thông báo kẾ hoách tổchức đàotạonộidộ Côngty'
$f = \"@SQL=\\"urn:schemas:httpmail:subject\\" like '%$query%' OR \\"urn:schemas:httpmail:textdescription\\" like '%$query%'\"
try { $i = $inbox.Items.Restrict($f); Write-Output $i.Count } catch { Write-Output $_.Exception.Message }