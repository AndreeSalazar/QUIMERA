param([switch]$MostrarVentana)
$ErrorActionPreference = 'Stop'
$raiz = Split-Path -Parent $PSScriptRoot
$linux = Join-Path (Split-Path -Parent $raiz) 'Linux Reconstruido'
$qemu = 'C:\Program Files\qemu\qemu-system-x86_64.exe'
$evidencia = Join-Path $raiz 'evidencias\chromium-real'
$disco = Join-Path $raiz 'vm\chromium-fuentes.qcow2'
if (!(Test-Path -LiteralPath $disco)) { throw 'Falta disco de fuentes' }
foreach ($puerto in @(15665,15666)) {
    $conexion = Get-NetTCPConnection -State Listen -LocalPort $puerto -ErrorAction SilentlyContinue
    if ($conexion) { throw "Puerto $puerto ocupado: se conserva la VM existente" }
}
$argumentos = @(
    '-name', '"ULTRA Chromium SUPER GRAFO"',
    '-machine', 'pc', '-accel', 'whpx', '-cpu', 'host', '-smp', '2', '-m', '4096',
    '-kernel', ('"' + (Join-Path $linux 'build\pc\bzImage-tiny64') + '"'),
    '-append', '"console=ttyS0,115200 rdinit=/init panic=-1 noapic quiet GLIBC_TUNABLES=glibc.cpu.hwcaps=SHSTK:glibc.cpu.x86_shstk=on"',
    '-drive', ('"file=' + (Join-Path $linux 'build\pc\disco-bmo.img') + ',format=raw,snapshot=on,if=virtio"'),
    '-drive', ('"file=' + $disco + ',format=qcow2,if=none,id=chromium"'),
    '-device', 'virtio-blk-pci,drive=chromium,serial=CHROMIUM-SOURCE',
    '-vga', 'virtio', '-nic', 'user,model=virtio-net-pci', '-display', $(if ($MostrarVentana) { 'gtk' } else { 'none' }),
    '-serial', 'tcp:127.0.0.1:15665,server=on,wait=off',
    '-qmp', 'tcp:127.0.0.1:15666,server=on,wait=off', '-no-reboot'
)
$proceso = Start-Process -FilePath $qemu -ArgumentList $argumentos -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $evidencia 'qemu.log')
@{ PID=$proceso.Id; Serial=15665; QMP=15666; DiscoFuentes=$disco; BaseModo='snapshot'; RAMMiB=4096 } |
    ConvertTo-Json | Set-Content -LiteralPath (Join-Path $raiz 'vm\sesion.json')
Write-Output "ULTRA QEMU PID=$($proceso.Id), consola=15665, QMP=15666"
