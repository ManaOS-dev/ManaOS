param(
    [string]$Path = "disk.img",
    [UInt64]$SizeBytes = 67108864
)

$ErrorActionPreference = "Stop"

$sectorSize = 512
$partitionEntryCount = 128
$partitionEntrySize = 128
$partitionEntryArrayBytes = $partitionEntryCount * $partitionEntrySize
$partitionEntrySectors = [UInt64]($partitionEntryArrayBytes / $sectorSize)
$totalSectors = [UInt64]($SizeBytes / $sectorSize)

if ($SizeBytes % $sectorSize -ne 0) {
    throw "disk image size must be sector aligned"
}

if ($totalSectors -lt 34) {
    throw "disk image must have at least 34 sectors"
}

function Write-LeUInt32 {
    param([byte[]]$Buffer, [int]$Offset, [UInt32]$Value)
    $bytes = [BitConverter]::GetBytes($Value)
    [Array]::Copy($bytes, 0, $Buffer, $Offset, 4)
}

function Write-LeUInt64 {
    param([byte[]]$Buffer, [int]$Offset, [UInt64]$Value)
    $bytes = [BitConverter]::GetBytes($Value)
    [Array]::Copy($bytes, 0, $Buffer, $Offset, 8)
}

function Get-Crc32 {
    param([byte[]]$Bytes)

    $crcMask = [UInt64]4294967295
    $crc = $crcMask
    $polynomial = [UInt64]3988292384
    foreach ($byte in $Bytes) {
        $crc = ($crc -bxor [UInt64]$byte) -band $crcMask
        for ($bit = 0; $bit -lt 8; $bit++) {
            if (($crc -band 1) -ne 0) {
                $crc = (($crc -shr 1) -bxor $polynomial) -band $crcMask
            } else {
                $crc = ($crc -shr 1) -band $crcMask
            }
        }
    }

    return [UInt32](($crc -bxor $crcMask) -band $crcMask)
}

function New-GptHeader {
    param(
        [UInt64]$CurrentLba,
        [UInt64]$BackupLba,
        [UInt64]$PartitionEntryLba,
        [UInt32]$PartitionArrayCrc32
    )

    $headerSize = 92
    $header = New-Object byte[] $sectorSize
    $signature = [Text.Encoding]::ASCII.GetBytes("EFI PART")
    [Array]::Copy($signature, 0, $header, 0, $signature.Length)
    Write-LeUInt32 $header 8 0x00010000
    Write-LeUInt32 $header 12 ([UInt32]$headerSize)
    Write-LeUInt64 $header 24 $CurrentLba
    Write-LeUInt64 $header 32 $BackupLba
    Write-LeUInt64 $header 40 34
    Write-LeUInt64 $header 48 ($totalSectors - 34)
    $diskGuid = [Guid]::Parse("11111111-2222-3333-4444-555555555555").ToByteArray()
    [Array]::Copy($diskGuid, 0, $header, 56, 16)
    Write-LeUInt64 $header 72 $PartitionEntryLba
    Write-LeUInt32 $header 80 ([UInt32]$partitionEntryCount)
    Write-LeUInt32 $header 84 ([UInt32]$partitionEntrySize)
    Write-LeUInt32 $header 88 $PartitionArrayCrc32

    $headerForCrc = New-Object byte[] $headerSize
    [Array]::Copy($header, 0, $headerForCrc, 0, $headerSize)
    Write-LeUInt32 $headerForCrc 16 0
    Write-LeUInt32 $header 16 (Get-Crc32 $headerForCrc)

    return $header
}

$image = New-Object byte[] $SizeBytes

$protectiveMbr = New-Object byte[] $sectorSize
$protectiveMbr[446 + 4] = 0xEE
$protectiveMbr[446 + 8] = 0x01
$protectiveSectorCount = $totalSectors - 1
$maxProtectiveSectorCount = [UInt64]4294967295
if ($protectiveSectorCount -gt $maxProtectiveSectorCount) {
    $protectiveSectorCount = $maxProtectiveSectorCount
}
Write-LeUInt32 $protectiveMbr (446 + 12) ([UInt32]$protectiveSectorCount)
$protectiveMbr[510] = 0x55
$protectiveMbr[511] = 0xAA
[Array]::Copy($protectiveMbr, 0, $image, 0, $sectorSize)

$partitionEntries = New-Object byte[] $partitionEntryArrayBytes
$partitionArrayCrc32 = Get-Crc32 $partitionEntries

$primaryPartitionEntryLba = [UInt64]2
$backupPartitionEntryLba = $totalSectors - 1 - $partitionEntrySectors
$lastLba = $totalSectors - 1

$primaryHeader = New-GptHeader `
    -CurrentLba 1 `
    -BackupLba $lastLba `
    -PartitionEntryLba $primaryPartitionEntryLba `
    -PartitionArrayCrc32 $partitionArrayCrc32
$backupHeader = New-GptHeader `
    -CurrentLba $lastLba `
    -BackupLba 1 `
    -PartitionEntryLba $backupPartitionEntryLba `
    -PartitionArrayCrc32 $partitionArrayCrc32

[Array]::Copy($primaryHeader, 0, $image, [int]($sectorSize * 1), $sectorSize)
[Array]::Copy(
    $partitionEntries,
    0,
    $image,
    [int]($sectorSize * $primaryPartitionEntryLba),
    $partitionEntryArrayBytes
)
[Array]::Copy(
    $partitionEntries,
    0,
    $image,
    [int]($sectorSize * $backupPartitionEntryLba),
    $partitionEntryArrayBytes
)
[Array]::Copy($backupHeader, 0, $image, [int]($sectorSize * $lastLba), $sectorSize)

[System.IO.File]::WriteAllBytes($Path, $image)
Write-Host "[disk ] Created GPT disk image: $Path ($SizeBytes bytes)"
