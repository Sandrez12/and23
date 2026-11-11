# Instala And23 en Windows: descarga la release (por defecto la última), comprueba su suma SHA-256 y copia and23.exe a una carpeta.
#
#   irm https://raw.githubusercontent.com/Sandrez12/and23/main/scripts/instalar.ps1 | iex
#
# Variables de entorno opcionales:
#   AND23_VERSION=v1.0.0    una versión concreta (por defecto la última)
#   AND23_DESTINO=carpeta   dónde copiar el binario (por defecto %LOCALAPPDATA%\And23\bin)
#   AND23_SIN_VERIFICAR=1   no exigir el archivo de sumas (solo para releases antiguas que no lo traen)
$ErrorActionPreference = "Stop"

$repo = "Sandrez12/and23"
$version = if ($env:AND23_VERSION) { $env:AND23_VERSION } else { "latest" }
$destino = if ($env:AND23_DESTINO) { $env:AND23_DESTINO } else { Join-Path $env:LOCALAPPDATA "And23\bin" }
$target = "x86_64-pc-windows-msvc"

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("and23-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    if ($version -eq "latest") {
        $info = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
        $version = $info.tag_name
    }
    $archivo = "and23-$version-$target.zip"
    $base = "https://github.com/$repo/releases/download/$version"
    Write-Host "Descargando And23 $version ($target)..."
    Invoke-WebRequest "$base/$archivo" -OutFile (Join-Path $tmp $archivo)

    $sumas = Join-Path $tmp "SHA256SUMS.txt"
    $hayRelease = $true
    try { Invoke-WebRequest "$base/SHA256SUMS.txt" -OutFile $sumas } catch { $hayRelease = $false }
    if ($hayRelease) {
        $linea = Get-Content $sumas | Where-Object { $_ -match [regex]::Escape($archivo) + "$" } | Select-Object -First 1
        $esperado = if ($linea) { ($linea -split "\s+")[0].ToLower() } else { "" }
        $obtenido = (Get-FileHash (Join-Path $tmp $archivo) -Algorithm SHA256).Hash.ToLower()
        if (-not $esperado -or $esperado -ne $obtenido) {
            throw "la suma SHA-256 de $archivo no coincide (esperada: $esperado, obtenida: $obtenido). No se instala."
        }
        Write-Host "Suma SHA-256 correcta."
    } elseif ($env:AND23_SIN_VERIFICAR -eq "1") {
        Write-Warning "esta release no trae SHA256SUMS.txt; se instala sin verificar (AND23_SIN_VERIFICAR=1)."
    } else {
        throw "no se pudo descargar SHA256SUMS.txt para verificar la descarga (AND23_SIN_VERIFICAR=1 lo omite)."
    }

    Expand-Archive (Join-Path $tmp $archivo) -DestinationPath $tmp
    New-Item -ItemType Directory -Force -Path $destino | Out-Null
    Copy-Item (Join-Path $tmp "and23-$version-$target\and23.exe") (Join-Path $destino "and23.exe") -Force
    Write-Host "Instalado en $destino\and23.exe"
    if (($env:PATH -split ";") -notcontains $destino) {
        Write-Host "aviso: $destino no está en tu PATH; añádelo para poder escribir and23."
    }
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
