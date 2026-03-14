# NTFS Internals Reference

This document explains the internal architecture of NTFS as it relates to defragmentation in RustDefrag.

## Clusters

The smallest unit of allocation in NTFS.

Common cluster sizes:
- 4 KB (most common for volumes up to 16 TB)
- 8 KB
- 16 KB
- 32 KB
- 64 KB (for very large volumes)

## Logical Cluster Number (LCN)

Represents the absolute physical position of a cluster on disk. LCN 0 is the first cluster on the volume.

## Virtual Cluster Number (VCN)

Represents the cluster offset within a specific file. VCN 0 is the first cluster of the file data.

The mapping `VCN â†’ LCN` is stored in the file's `DATA` attribute within the MFT as a compressed run list.

## Master File Table (MFT)

The MFT is the backbone of NTFS. Every file and directory is represented by one or more MFT records.

Each record contains:
- File attributes (timestamps, security descriptor, data runs)
- The cluster run list for the file's data
- Extended attributes

The MFT itself occupies clusters on the volume and can grow dynamically.

## Cluster Runs

Files are stored as a list of contiguous runs.

Example of a fragmented file:
```
Run 1: LCN=100, Length=10   (clusters 100-109)
Run 2: LCN=500, Length=8    (clusters 500-507)
```

A contiguous file has exactly one run:
```
Run 1: LCN=200, Length=18   (clusters 200-217)
```

## Retrieval Pointers

Windows returns run lists via the `FSCTL_GET_RETRIEVAL_POINTERS` IOCTL.

The output structure contains an array of `{NextVCN, LCN}` pairs. The run length is computed as `NextVCN - PreviousVCN`.

## Volume Bitmap

The volume bitmap is a bit-array where bit `n` corresponds to cluster `n`. A set bit (1) means the cluster is allocated; a clear bit (0) means it is free.

Retrieved via: `FSCTL_GET_VOLUME_BITMAP`

## Cluster Relocation

Clusters are moved using: `FSCTL_MOVE_FILE`

The `MOVE_FILE_DATA` structure specifies:
- `FileHandle` â€” handle to the file
- `StartingVcn` â€” the VCN of the first cluster to move
- `StartingLcn` â€” the destination LCN
- `ClusterCount` â€” number of clusters to move

## Protected System Files

These files must never be relocated by a defragmenter:
- `$MFT` â€” Master File Table
- `$MFTMirr` â€” MFT mirror (backup)
- `$LogFile` â€” NTFS journal
- `$Volume` â€” Volume metadata
- `$AttrDef` â€” Attribute definitions
- `$Bitmap` â€” Volume bitmap
- `$Boot` â€” Boot sector
- `$BadClus` â€” Bad cluster list
- `$Secure` â€” Security descriptors
- `$UpCase` â€” Unicode uppercase table
- `$Extend` â€” Extended metadata directory

## Runtime Lock Files

These files must be skipped at runtime:
- `pagefile.sys` â€” Windows virtual memory
- `hiberfil.sys` â€” Hibernation image
- `swapfile.sys` â€” Modern standby swap

Repository: https://github.com/arafat877/rust-defrag

