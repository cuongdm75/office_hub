# poem_test.ps1
$text = Get-Content -Path ".\poem.txt" -Encoding UTF8
$title = $text[0]
$line1 = $text[1]
$line2 = $text[2]
$line3 = $text[3]
$line4 = $text[4]
$author = $text[5]

Write-Host "Khoi dong Microsoft Word qua COM Automation..." -ForegroundColor Cyan

$word = New-Object -ComObject Word.Application
$word.Visible = $true

$doc = $word.Documents.Add()
$selection = $word.Selection

Write-Host "Dinh dang le trang (Margins)..." -ForegroundColor Yellow
$doc.PageSetup.TopMargin = $word.CentimetersToPoints(2.5)
$doc.PageSetup.BottomMargin = $word.CentimetersToPoints(2.5)
$doc.PageSetup.LeftMargin = $word.CentimetersToPoints(3.0)
$doc.PageSetup.RightMargin = $word.CentimetersToPoints(2.0)

# Title
$selection.Font.Name = "Times New Roman"
$selection.Font.Size = 24
$selection.Font.Bold = $true
$selection.Font.Color = 12611584 # Blue
$selection.ParagraphFormat.Alignment = 1 # Center
$selection.TypeText($title)
$selection.TypeParagraph()
$selection.TypeParagraph()

# Poem
Write-Host "Dang viet bai tho..." -ForegroundColor Yellow
$selection.Font.Size = 16
$selection.Font.Bold = $false
$selection.Font.Italic = $true
$selection.Font.Color = 0 # Black
$selection.ParagraphFormat.Alignment = 1 # Center

$poemLines = @($line1, $line2, $line3, $line4)

foreach ($line in $poemLines) {
    $selection.TypeText($line)
    $selection.TypeParagraph()
    Start-Sleep -Milliseconds 300
}

$selection.TypeParagraph()

# Author
$selection.Font.Italic = $false
$selection.Font.Bold = $true
$selection.Font.Size = 14
$selection.ParagraphFormat.Alignment = 2 # Right
$selection.TypeText($author)

Write-Host "Hoan tat! Vui long kiem tra man hinh Word." -ForegroundColor Green
