# NTFS Internals Reference

This document explains NTFS internals relevant to RustDefrag.

## Clusters

A cluster is the smallest allocation unit in NTFS.

Common cluster sizes:
- 4 KB (common)
- 8 KB
- 16 KB
- 32 KB
- 64 KB (very large volumes)

## Logical Cluster Number (LCN)

Represents the absolute physical cluster location on disk.

## Virtual Cluster Number (VCN)

Represents the cluster offset within a specific file.

Mapping is `VCN -> LCN` and is stored in the file `DATA` attribute as compressed runs.

## Master File Table (MFT)

The MFT is the core NTFS metadata table. Every file and directory is represented by one or more records.

Each record includes:
- file attributes (timestamps, security descriptor, data runs)
- cluster run list for file data
- extended attributes

## Cluster Runs

Files are stored as contiguous runs.

Example fragmented file:

```text
Run 1: LCN=100, Length=10   (clusters 100-109)
Run 2: LCN=500, Length=8    (clusters 500-507)
```

Contiguous file:

```text
Run 1: LCN=200, Length=18   (clusters 200-217)
```

## Retrieval Pointers

Windows returns run lists via `FSCTL_GET_RETRIEVAL_POINTERS`.

Output includes `{NextVCN, LCN}` pairs. Run length is `NextVCN - PreviousVCN`.

## Volume Bitmap

The volume bitmap is a bit array where bit `n` represents cluster `n`.

- `1` = allocated
- `0` = free

Retrieved via `FSCTL_GET_VOLUME_BITMAP`.

## Cluster Relocation

Clusters are moved via `FSCTL_MOVE_FILE`.

`MOVE_FILE_DATA` contains:
- `FileHandle` - handle to file
- `StartingVcn` - first VCN to move
- `StartingLcn` - destination LCN
- `ClusterCount` - number of clusters

## Protected System Files

These must never be relocated:
- `$MFT` - Master File Table
- `$MFTMirr` - MFT mirror
- `$LogFile` - NTFS journal
- `$Volume` - volume metadata
- `$AttrDef` - attribute definitions
- `$Bitmap` - volume bitmap
- `$Boot` - boot sector
- `$BadClus` - bad cluster list
- `$Secure` - security descriptors
- `$UpCase` - uppercase table
- `$Extend` - extended metadata directory

## Runtime Lock Files

These are usually in use and should be skipped:
- `pagefile.sys` - virtual memory file
- `hiberfil.sys` - hibernation image
- `swapfile.sys` - swap support file

Repository: https://github.com/arafat877/rust-defrag
