param(
    [string]$Path = "disk.img",
    [UInt64]$SizeBytes = 67108864,
    [string]$SmokeDemoElfPath = "target\userland\x86_64-unknown-none\debug\smoke_demo"
)

$ErrorActionPreference = "Stop"

$sectorSize = 512
$partitionEntryCount = 128
$partitionEntrySize = 128
$partitionEntryArrayBytes = $partitionEntryCount * $partitionEntrySize
$partitionEntrySectors = [UInt64]($partitionEntryArrayBytes / $sectorSize)
$totalSectors = [UInt64]($SizeBytes / $sectorSize)
$firstPartitionLba = [UInt64]2048
$lastPartitionLba = $totalSectors - 34

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

function Write-LeUInt16 {
    param([byte[]]$Buffer, [int]$Offset, [UInt16]$Value)
    $bytes = [BitConverter]::GetBytes($Value)
    [Array]::Copy($bytes, 0, $Buffer, $Offset, 2)
}

function Write-LeUInt64 {
    param([byte[]]$Buffer, [int]$Offset, [UInt64]$Value)
    $bytes = [BitConverter]::GetBytes($Value)
    [Array]::Copy($bytes, 0, $Buffer, $Offset, 8)
}

function Write-AsciiField {
    param([byte[]]$Buffer, [int]$Offset, [int]$Length, [string]$Value)
    for ($index = 0; $index -lt $Length; $index++) {
        $Buffer[$Offset + $index] = 0x20
    }

    $bytes = [Text.Encoding]::ASCII.GetBytes($Value)
    $copyLength = [Math]::Min($bytes.Length, $Length)
    [Array]::Copy($bytes, 0, $Buffer, $Offset, $copyLength)
}

function Write-ShortNameField {
    param([byte[]]$Buffer, [int]$Offset, [string]$Value)
    if ($Value.Length -ne 11) {
        throw "short FAT name field must contain exactly 11 characters"
    }

    $bytes = [Text.Encoding]::ASCII.GetBytes($Value)
    [Array]::Copy($bytes, 0, $Buffer, $Offset, 11)
}

function Write-DirectoryEntry {
    param(
        [byte[]]$Image,
        [int]$Offset,
        [string]$ShortName,
        [byte]$Attribute,
        [UInt32]$FirstCluster,
        [UInt32]$FileSize
    )

    Write-ShortNameField $Image $Offset $ShortName
    $Image[$Offset + 11] = $Attribute
    Write-LeUInt16 $Image ($Offset + 20) ([UInt16](($FirstCluster -shr 16) -band 0xFFFF))
    Write-LeUInt16 $Image ($Offset + 26) ([UInt16]($FirstCluster -band 0xFFFF))
    Write-LeUInt32 $Image ($Offset + 28) $FileSize
}

function Write-LongFileNameEntry {
    param(
        [byte[]]$Image,
        [int]$Offset,
        [string]$Name
    )

    $Image[$Offset] = 0x41
    $Image[$Offset + 11] = 0x0F
    $Image[$Offset + 13] = 0
    Write-LeUInt16 $Image ($Offset + 26) 0

    $slots = @(1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30)
    for ($index = 0; $index -lt $slots.Length; $index++) {
        if ($index -lt $Name.Length) {
            $codeUnit = [UInt16][char]$Name[$index]
        } elseif ($index -eq $Name.Length) {
            $codeUnit = [UInt16]0
        } else {
            $codeUnit = [UInt16]0xFFFF
        }
        Write-LeUInt16 $Image ($Offset + $slots[$index]) $codeUnit
    }
}

function Write-FileSystemInformationSector {
    param(
        [byte[]]$Image,
        [int]$Offset,
        [UInt32]$FreeClusterCount,
        [UInt32]$NextFreeCluster
    )

    Write-LeUInt32 $Image $Offset 0x41615252
    Write-LeUInt32 $Image ($Offset + 484) 0x61417272
    Write-LeUInt32 $Image ($Offset + 488) $FreeClusterCount
    Write-LeUInt32 $Image ($Offset + 492) $NextFreeCluster
    Write-LeUInt32 $Image ($Offset + 508) 2857697280
}

function Write-GptGuid {
    param([byte[]]$Buffer, [int]$Offset, [string]$Value)
    $bytes = [Guid]::Parse($Value).ToByteArray()
    [Array]::Copy($bytes, 0, $Buffer, $Offset, 16)
}

function Write-GptPartitionName {
    param([byte[]]$Buffer, [int]$Offset, [string]$Value)
    $bytes = [Text.Encoding]::Unicode.GetBytes($Value)
    $length = [Math]::Min($bytes.Length, 72)
    [Array]::Copy($bytes, 0, $Buffer, $Offset, $length)
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

function Write-TestPartitionEntry {
    param([byte[]]$PartitionEntries)

    if ($firstPartitionLba -gt $lastPartitionLba) {
        throw "disk image is too small for the test partition"
    }

    Write-GptGuid $PartitionEntries 0 "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7"
    Write-GptGuid $PartitionEntries 16 "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
    Write-LeUInt64 $PartitionEntries 32 $firstPartitionLba
    Write-LeUInt64 $PartitionEntries 40 $lastPartitionLba
    Write-GptPartitionName $PartitionEntries 56 "ManaOS Data"
}

function Write-FileAllocationTable32BootSector {
    param([byte[]]$Image)
    if (-not (Test-Path -LiteralPath $SmokeDemoElfPath)) {
        throw "smoke_demo ELF not found at $SmokeDemoElfPath; run cargo build first"
    }
    $smokeDemoElf = [System.IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $SmokeDemoElfPath))

    $partitionSectors = $lastPartitionLba - $firstPartitionLba + 1
    if ($partitionSectors -gt [UInt64][UInt32]::MaxValue) {
        throw "test FAT32 partition is too large"
    }

    $reservedSectorCount = [UInt16]32
    $fileAllocationTableCount = [byte]2
    $fileAllocationTableSize = [UInt32]1024
    $sectorsPerCluster = [byte]1
    $metadataSectors = [UInt64]$reservedSectorCount + ([UInt64]$fileAllocationTableCount * [UInt64]$fileAllocationTableSize)
    if ($partitionSectors -le $metadataSectors) {
        throw "test FAT32 partition is too small"
    }

    $bootSector = New-Object byte[] $sectorSize
    $bootSector[0] = 0xEB
    $bootSector[1] = 0x58
    $bootSector[2] = 0x90
    Write-AsciiField $bootSector 3 8 "MANAOS"
    Write-LeUInt16 $bootSector 11 ([UInt16]$sectorSize)
    $bootSector[13] = $sectorsPerCluster
    Write-LeUInt16 $bootSector 14 $reservedSectorCount
    $bootSector[16] = $fileAllocationTableCount
    Write-LeUInt16 $bootSector 17 0
    Write-LeUInt16 $bootSector 19 0
    $bootSector[21] = 0xF8
    Write-LeUInt16 $bootSector 22 0
    Write-LeUInt16 $bootSector 24 63
    Write-LeUInt16 $bootSector 26 255
    Write-LeUInt32 $bootSector 28 ([UInt32]$firstPartitionLba)
    Write-LeUInt32 $bootSector 32 ([UInt32]$partitionSectors)
    Write-LeUInt32 $bootSector 36 $fileAllocationTableSize
    Write-LeUInt16 $bootSector 40 0
    Write-LeUInt16 $bootSector 42 0
    Write-LeUInt32 $bootSector 44 2
    Write-LeUInt16 $bootSector 48 1
    Write-LeUInt16 $bootSector 50 6
    $bootSector[64] = 0x80
    $bootSector[66] = 0x29
    Write-LeUInt32 $bootSector 67 0x4D414E41
    Write-AsciiField $bootSector 71 11 "MANAOS"
    Write-AsciiField $bootSector 82 8 "FAT32"
    $bootSector[510] = 0x55
    $bootSector[511] = 0xAA

    [Array]::Copy($bootSector, 0, $Image, [int]($sectorSize * $firstPartitionLba), $sectorSize)

    $firstFileAllocationTableOffset = [int](($firstPartitionLba + $reservedSectorCount) * $sectorSize)
    $secondFileAllocationTableOffset = [int](($firstPartitionLba + $reservedSectorCount + $fileAllocationTableSize) * $sectorSize)
    $smokeDemoClusterCount = [UInt32][Math]::Ceiling($smokeDemoElf.Length / [double]$sectorSize)
    if ($smokeDemoClusterCount -eq 0) {
        throw "smoke_demo ELF must not be empty"
    }
    $smokeDemoFirstCluster = [UInt32]5
    $smokeDemoLastCluster = [UInt32]($smokeDemoFirstCluster + $smokeDemoClusterCount - 1)
    $usedDataClusters = [UInt32](3 + $smokeDemoClusterCount)
    $dataClusterCount = [UInt32](($partitionSectors - $metadataSectors) / [UInt64]$sectorsPerCluster)
    if ($usedDataClusters -ge $dataClusterCount) {
        throw "test FAT32 partition does not have enough data clusters"
    }
    $nextFreeCluster = [UInt32]($smokeDemoLastCluster + 1)
    $freeClusterCount = [UInt32]($dataClusterCount - $usedDataClusters)

    foreach ($offset in @($firstFileAllocationTableOffset, $secondFileAllocationTableOffset)) {
        Write-LeUInt32 $Image $offset 0x0FFFFFF8
        Write-LeUInt32 $Image ($offset + 4) 0x0FFFFFFF
        Write-LeUInt32 $Image ($offset + 8) 0x0FFFFFFF
        Write-LeUInt32 $Image ($offset + 12) 0x0FFFFFFF
        Write-LeUInt32 $Image ($offset + 16) 0x0FFFFFFF
        for ($cluster = $smokeDemoFirstCluster; $cluster -le $smokeDemoLastCluster; $cluster++) {
            $entryOffset = $offset + ([int]$cluster * 4)
            if ($cluster -eq $smokeDemoLastCluster) {
                Write-LeUInt32 $Image $entryOffset 0x0FFFFFFF
            } else {
                Write-LeUInt32 $Image $entryOffset ([UInt32]($cluster + 1))
            }
        }
    }

    $rootDirectoryOffset = [int](($firstPartitionLba + $metadataSectors) * $sectorSize)
    $fileBytes = [Text.Encoding]::ASCII.GetBytes("hello from FAT32`r`n")
    Write-DirectoryEntry $Image $rootDirectoryOffset "HELLO   TXT" 0x20 3 ([UInt32]$fileBytes.Length)
    Write-DirectoryEntry $Image ($rootDirectoryOffset + 32) "BIN        " 0x10 4 0

    $fileDataOffset = [int](($firstPartitionLba + $metadataSectors + 1) * $sectorSize)
    [Array]::Copy($fileBytes, 0, $Image, $fileDataOffset, $fileBytes.Length)

    $binDirectoryOffset = [int](($firstPartitionLba + $metadataSectors + 2) * $sectorSize)
    Write-LongFileNameEntry $Image $binDirectoryOffset "smoke_demo"
    Write-DirectoryEntry $Image ($binDirectoryOffset + 32) "SMOKED~1   " 0x20 $smokeDemoFirstCluster ([UInt32]$smokeDemoElf.Length)

    $smokeDemoDataOffset = [int](($firstPartitionLba + $metadataSectors + 3) * $sectorSize)
    [Array]::Copy($smokeDemoElf, 0, $Image, $smokeDemoDataOffset, $smokeDemoElf.Length)

    [Array]::Copy($bootSector, 0, $Image, [int](($firstPartitionLba + 6) * $sectorSize), $sectorSize)
    Write-FileSystemInformationSector `
        -Image $Image `
        -Offset ([int](($firstPartitionLba + 1) * $sectorSize)) `
        -FreeClusterCount $freeClusterCount `
        -NextFreeCluster $nextFreeCluster
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
Write-TestPartitionEntry $partitionEntries
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
Write-FileAllocationTable32BootSector $image

[System.IO.File]::WriteAllBytes($Path, $image)
Write-Host "[disk ] Created GPT/FAT32 disk image: $Path ($SizeBytes bytes)"
