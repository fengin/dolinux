Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = 'HighQuality'
$g.Clear([System.Drawing.Color]::Transparent)

# Lightning bolt polygon points
$points = @(
    (New-Object System.Drawing.PointF(580, 20)),
    (New-Object System.Drawing.PointF(200, 530)),
    (New-Object System.Drawing.PointF(450, 530)),
    (New-Object System.Drawing.PointF(350, 1004)),
    (New-Object System.Drawing.PointF(830, 420)),
    (New-Object System.Drawing.PointF(560, 420)),
    (New-Object System.Drawing.PointF(700, 20))
)

# Gradient fill: golden yellow to orange
$rect = New-Object System.Drawing.RectangleF(150, 0, 700, 1024)
$color1 = [System.Drawing.Color]::FromArgb(255, 255, 215, 0)
$color2 = [System.Drawing.Color]::FromArgb(255, 255, 140, 0)
$brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush($rect, $color1, $color2, [System.Drawing.Drawing2D.LinearGradientMode]::Vertical)

$g.FillPolygon($brush, $points)

# Subtle dark outline for definition
$outlineColor = [System.Drawing.Color]::FromArgb(100, 180, 120, 0)
$pen = New-Object System.Drawing.Pen($outlineColor, 6)
$g.DrawPolygon($pen, $points)

$g.Dispose()
$brush.Dispose()
$pen.Dispose()
$bmp.Save("d:\code\career\dolinux\app-icon.png", [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "Icon created with transparent background"
