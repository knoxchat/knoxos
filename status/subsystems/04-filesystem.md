← [Status index](../README.md) · [Do this first](../next.md)

# 4. Filesystem & Storage

**Grade: Wired (100%)** · `vfs.rs`, `block.rs`, `virtio_blk.rs`, `ext4.rs`, `fat32.rs`, `persist.rs`, `partition.rs`, `page_cache.rs`, `ahci.rs`, `nvme.rs`

### Live
- [x] In-memory VFS (FHS tree, path walk, fds, metadata)
- [x] procfs / sysfs / devfs / tmpfs (virtual)
- [x] ATA PIO IDENTIFY/READ/WRITE (`block.rs`, ports `0x1F0` / `0x170`)
- [x] VirtIO-blk: PCI, virtqueue, guest-physical DMA + bounce buffer, sector R/W (QEMU)
- [x] ext4: superblock at LBA 2, inode table, extents, read/write via block layer
- [x] FAT32: BPB, FAT, clusters, LFN
- [x] GPT/MBR parse (`partition.rs`)
- [x] `persist.rs` blob store on VirtIO-blk (`KNOXPERSIST`)
- [x] **Gate C1 persist roundtrip** — write `/var/lib/knoxos/gate_c1`, unlink from RAM VFS, restore from VirtIO-blk
- [x] **Gate C2 persist WAL** — committed journal record survives simulated crash; uncommitted is discarded (`GATE_C2 journal recovered`)
- [x] **Gate C3 page cache writeback** — dirty 4 KiB page overlays backing file at offset; sibling pages survive (`GATE_C3 writeback complete`)
- [x] **Gate C4 AHCI DMA** — command list + PRDT write then read a marker sector (`GATE_C4 ahci dma complete`)
- [x] **Gate C5 NVMe DMA** — admin + I/O queues; PRP1 bounce write/read round-trip (`GATE_C5 nvme dma complete`)
- [x] **Gate C6 VFS root persist** — `/etc` (and the rest of the RAM namespace except virtual FS) round-trips through VirtIO-blk (`GATE_C6 vfs persist`)
- [x] **Gate C7 `/bin` persist** — a file under `/bin` round-trips through VirtIO-blk (`GATE_C7 bin persist`).
- [x] **Gate C10 virtual FS mount points** — `/dev` `/proc` `/sys` `/run` `/tmp` directories round-trip; generated files under them stay virtual (`GATE_C10 virtfs persist`)
- [x] 4 MB ramdisk always created

### Wired / partial
- [x] Page cache is an in-RAM `BTreeMap`; write-back overlays dirty pages via `pwrite_file` (C3)
- [x] Mount table: `unshare(CLONE_NEWNS)` isolates bind mounts (N1); ext4, ext2, FAT32 (`vfat`), ISO9660, NTFS, exFAT, Btrfs, XFS, and ZFS really mount (`GATE_FAT32 mount`, `GATE_ISO9660 mount`, `GATE_NTFS mount`, `GATE_EXFAT mount`, `GATE_BTRFS mount`, `GATE_XFS mount`, `GATE_ZFS mount`)
- [x] flock — Ring 3 `flock(LOCK_EX)` then `LOCK_UN` on a VFS file (R3). xattr `setxattr`/`getxattr` live on in-kernel maps (U2)
- [x] `sendfile` — Ring 3 file-to-pipe byte copy (S2)
- [x] `tee` — Ring 3 pipe duplicate without consuming the source (S3)
- [x] `copy_file_range` — Ring 3 file-to-file byte copy (T2)
- [x] `vmsplice` — Ring 3 user pages spliced into a pipe (T3)
- [x] `statx` — Ring 3 `stx_size` from a VFS file (U3)
- [x] `fallocate` — Ring 3 extends a VFS file; `statx` sees the new size (V2)
- [x] `utimensat` — Ring 3 sets inode mtime; `statx` reports `stx_mtime` (V3)
- [x] OverlayFS — VFS-backed lower+upper merge + whiteout, mount-ns isolated (W1)
- [x] `umask` — applied on `open(O_CREAT)` and `mkdir` (W2)
- [x] `symlink`/`readlink` — Ring 3 target round-trip (W3)
- [x] Hard links — two names share one inode; write via one is visible via the other; unlink decrements nlink (X1)
- [x] `rename` — Ring 3 write, rename, read from the new path (X2)
- [x] `truncate` — Ring 3 shrinks a VFS file; `statx` sees the new size (X3)
- [x] `chmod` — mode 0400: owner can read but not write; other cannot read (Y1)
- [x] `chown` — Ring 3 sets uid 1000; `statx` reports `stx_uid` (Y2)
- [x] `mkdir` — Ring 3 creates a directory; `statx` reports `S_IFDIR` (Y3)
- [x] `rmdir` — empty directory is removed; non-empty fails ENOTEMPTY (Z1)
- [x] `unlink` — Ring 3 create then unlink; subsequent `open` fails (Z2)
- [x] `chdir`/`getcwd` — Ring 3 `getcwd` matches the `chdir` target (Z3)
- [x] `mkfifo` — named FIFO write/read round-trip through the FIFO buffer (AA1)
- [x] `fchdir` — Ring 3 `fchdir` then `getcwd` (AA2)
- [x] `access` — Ring 3 `access(F_OK)` on a created file; missing path fails (AA3)
- [x] `getdents64` — VFS `list_dir` includes a created child (`GATE_AB1 getdents`)
- [x] `fcntl` — `F_SETFD`/`F_GETFD` CLOEXEC; `dup` clears cloexec (`GATE_AC1 fcntl`)
- [x] `pread64` — Ring 3 reads at offset without moving the fd position (AC2)
- [x] `fstat` — reports the written VFS size (`GATE_AD1 fstat`)
- [x] `pwrite64` — Ring 3 writes at offset without moving the fd position (AD2)
- [x] `writev` — scatter-gather write of two iovecs reads back `xy` (`GATE_AE1 writev`)
- [x] `ftruncate` — Ring 3 shrinks an fd; `statx` sees the new size (AE2)
- [x] `readv` — scatter-gather read of `xy` fills two iovecs (`GATE_AF1 readv`)
- [x] `lseek` — Ring 3 writes `xy`, seeks to 1, reads `y` (AF2)
- [x] `fsync` — succeeds on a written VFS fd; bad fd is EBADF (`GATE_AG1 fsync`)
- [x] `fdatasync` — Ring 3 write then `fdatasync` (`GATE_AG2 fdatasync`)
- [x] `syncfs` — succeeds on a written VFS fd; bad fd is EBADF (`GATE_AH1 syncfs`)
- [x] `sync` — Ring 3 write then `sync` (`GATE_AH2 sync`)
- [x] `fchmod` — sets mode 0400 on a written VFS fd; bad fd is EBADF (`GATE_AI1 fchmod`)
- [x] `fchown` — Ring 3 sets uid 1000 via fd; `statx` reports `stx_uid` (`GATE_AI2 fchown`)
- [x] `fstatfs` — reports `f_bsize == 4096` on a written VFS fd; bad fd is EBADF (`GATE_AJ1 fstatfs`)
- [x] `statfs` — Ring 3 `statfs("/tmp")` reports `f_bsize == 4096` (`GATE_AJ2 statfs`)
- [x] `fsetxattr`/`fgetxattr` — resolve fd to inode; round-trip `user.knox`; bad fd is EBADF (`GATE_AK1 fsetxattr`)
- [x] `flistxattr`/`fremovexattr` — list contains `user.knox` after set; remove clears it; bad fd is EBADF (`GATE_AL1 flistxattr`)
- [x] `listxattr`/`removexattr` — path list contains `user.knox` after set; remove clears it; missing path is ENOENT (`GATE_AM1 listxattr`)
- [x] `faccessat` — dirfd-relative `F_OK` succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AN1 faccessat`)
- [x] `mkdirat` — dirfd-relative create succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AO1 mkdirat`)
- [x] `unlinkat` — dirfd-relative unlink succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AP1 unlinkat`)
- [x] `renameat` — dirfd-relative rename succeeds; missing source is ENOENT; bad dirfd is EBADF (`GATE_AQ1 renameat`)
- [x] `linkat` — dirfd-relative hard link succeeds; missing source is ENOENT; bad dirfd is EBADF (`GATE_AR1 linkat`)
- [x] `symlinkat` — dirfd-relative symlink succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AS1 symlinkat`)
- [x] `readlinkat` — dirfd-relative read returns the target; missing child is ENOENT; bad dirfd is EBADF (`GATE_AT1 readlinkat`)
- [x] `mknodat` — dirfd-relative create succeeds; missing parent is ENOENT; bad dirfd is EBADF (`GATE_AU1 mknodat`)
- [x] `fchmodat` — dirfd-relative chmod 0400 succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_AV1 fchmodat`)
- [x] `execveat` — dirfd-relative exec succeeds; missing child is ENOENT; bad dirfd is EBADF (`GATE_BE1 execveat`)
- [x] NTFS — boot + MFT `$DATA` through the ramdisk block layer (`GATE_NTFS block`). `mount -t ntfs` reads `$DATA` (`GATE_NTFS mount`)
- [x] `fsync`/`fdatasync` — flush dirty page-cache pages, re-persist the fd's VFS file through the WAL, and issue VirtIO-blk FLUSH

### Stub
- [x] **AHCI** — HBA reset, port start, command-list + PRDT DMA read/write (C4); IDENTIFY parsed
- [x] **NVMe** — controller enable, admin/I/O queues, PRP bounce DMA read/write (C5)
- [x] exFAT — VBR + FAT + directory `$DATA` through the ramdisk (`GATE_EXFAT block`). `mount -t exfat` reads `$DATA` (`GATE_EXFAT mount`). CIFS SMB2 WRITE/READ (`GATE_CIFS smb2`). ZFS uberblock + Fletcher-4 (`GATE_ZFS block`). `mount -t zfs` reads `$DATA` (`GATE_ZFS mount`).
- [x] Btrfs — superblock + inode + extent `$DATA` through the ramdisk (`GATE_BTRFS block`). `mount -t btrfs` reads `$DATA` (`GATE_BTRFS mount`).
- [x] XFS — superblock + inode + extent `$DATA` through the ramdisk (`GATE_XFS block`). `mount -t xfs` reads `$DATA` (`GATE_XFS mount`).
- [x] JBD2 journal replay — descriptor + data + commit applied; torn descriptor discarded (`GATE_C8 jbd2 replay`). ext4 mount calls the same walker.
- [x] ext4 default root — second virtio-blk (`mkfs.ext4`) mounted at `/` (`GATE_C9 ext4 root`). Generated files under `/dev` `/proc` `/sys` `/run` `/tmp` stay virtual; the mount-point directories persist (C10).
- [x] OverlayFS — VFS-backed lower+upper merge + whiteout, isolated to the caller's mount ns (**Gate W1**). FUSE LOOKUP+READ (`GATE_FUSE lookup`). FUSE WRITE then READ (`GATE_FUSE write`). FUSE UNLINK (`GATE_FUSE unlink`). FUSE MKDIR LOOKUP `S_IFDIR` (`GATE_FUSE mkdir`). FUSE RMDIR (`GATE_FUSE rmdir`). FUSE RENAME (`GATE_FUSE rename`). FUSE SYMLINK LOOKUP `S_IFLNK` (`GATE_FUSE symlink`). FUSE LINK same inode (`GATE_FUSE link`). FUSE CREATE LOOKUP `S_IFREG` (`GATE_FUSE create`). FUSE GETATTR size+`S_IFREG` (`GATE_FUSE getattr`). FUSE SETATTR size (`GATE_FUSE setattr`). FUSE READDIR lists names (`GATE_FUSE readdir`). FUSE MKNOD LOOKUP `S_IFCHR` (`GATE_FUSE mknod`). FUSE STATFS `bsize` 4096 (`GATE_FUSE statfs`). FUSE SETXATTR then GETXATTR (`GATE_FUSE setxattr`). FUSE GETXATTR missing name fails (`GATE_FUSE getxattr`). FUSE LISTXATTR includes the stored name (`GATE_FUSE listxattr`). FUSE REMOVEXATTR drops it (`GATE_FUSE removexattr`). FUSE ACCESS grants file bits from mode (`GATE_FUSE access`). FUSE FSYNC after WRITE (`GATE_FUSE fsync`). FUSE OPEN returns fh=nodeid (`GATE_FUSE open`). FUSE RELEASE after OPEN (`GATE_FUSE release`). FUSE FLUSH after WRITE (`GATE_FUSE flush`). FUSE FSYNCDIR of a directory (`GATE_FUSE fsyncdir`). FUSE OPENDIR returns fh=nodeid (`GATE_FUSE opendir`). FUSE RELEASEDIR after OPENDIR (`GATE_FUSE releasedir`). FUSE GETLK of a known file with no lock is F_UNLCK (`GATE_FUSE getlk`). FUSE SETLK F_WRLCK is visible to GETLK from another pid (`GATE_FUSE setlk`). FUSE SETLKW F_WRLCK is visible to GETLK (`GATE_FUSE setlkw`). FUSE BMAP maps block 0 to nodeid (`GATE_FUSE bmap`). FUSE DESTROY of a known mount (`GATE_FUSE destroy`). FUSE IOCTL echoes cmd (`GATE_FUSE ioctl`). FUSE POLL of a known file with data is POLLIN (`GATE_FUSE poll`). FUSE FALLOCATE extends size (`GATE_FUSE fallocate`). FUSE READDIRPLUS lists names with size (`GATE_FUSE readdirplus`). FUSE RENAME2 moves then NOREPLACE fails (`GATE_FUSE rename2`). FUSE LSEEK SEEK_SET 0 then SEEK_END (`GATE_FUSE lseek`). FUSE COPY_FILE_RANGE copies bytes (`GATE_FUSE copy`). FUSE SYNCFS of a known mount (`GATE_FUSE syncfs`). FUSE STATX size and `S_IFREG` (`GATE_FUSE statx`). FUSE INIT returns major 7 (`GATE_FUSE init`). FUSE FORGET of a known nodeid (`GATE_FUSE forget`). NFS LOOKUPP of a child is the export root (`GATE_NFS lookupp`). NFS RESTOREFH restores SAVEFH; missing is NOFILEHANDLE (`GATE_NFS restorefh`). NFS PUTPUBFH after LOOKUP is the export root (`GATE_NFS putpubfh`). NFS VERIFY matching size is OK; mismatch is NOT_SAME (`GATE_NFS verify`). NFS NVERIFY mismatch is OK; match is SAME (`GATE_NFS nverify`). NFS SETCLIENTID_CONFIRM of the issued client_id (`GATE_NFS confirm`). NFS CLOSE of the issued OPEN (`GATE_NFS close`). NFS RENEW of a confirmed client_id (`GATE_NFS renew`). NFS LOCK of a free range is OK; overlap is DENIED (`GATE_NFS lock`). NFS LOCKU of a held range is OK; missing is DENIED (`GATE_NFS unlock`). NFS LOCKT of a free range is OK; overlap is DENIED (`GATE_NFS lockt`). NFS RELEASE_LOCKOWNER drops the owner's locks (`GATE_NFS lockowner`). NFS OPEN_CONFIRM of the issued OPEN (`GATE_NFS openconfirm`). NFS OPEN_DOWNGRADE to a subset (`GATE_NFS downgrade`). NFS SECINFO of a known name is AUTH_SYS (`GATE_NFS secinfo`). NFS OPENATTR createdir (`GATE_NFS openattr`). NFS DELEGPURGE of a confirmed client_id (`GATE_NFS delegpurge`). NFS DELEGRETURN of an OPEN state_id (`GATE_NFS delegreturn`). NFS EXCHANGE_ID of a client_name (`GATE_NFS exchangeid`). NFS CREATE_SESSION of that client_id (`GATE_NFS session`). NFS DESTROY_SESSION of that session (`GATE_NFS destroysession`). NFS DESTROY_CLIENTID after DESTROY_SESSION (`GATE_NFS destroyclientid`). NFS SEQUENCE of that session (`GATE_NFS sequence`). NFS RECLAIM_COMPLETE of that session (`GATE_NFS reclaim`). NFS4 COMPOUND PUTROOTFH+LOOKUP+READ (`GATE_NFS compound`). NFS WRITE then READ (`GATE_NFS write`). NFS REMOVE then LOOKUP `NFS4ERR_NOENT` (`GATE_NFS remove`). NFS RENAME then LOOKUP (`GATE_NFS rename`). NFS CREATE then LOOKUP (`GATE_NFS create`). NFS LINK then LOOKUP (`GATE_NFS link`). NFS CREATE NF4LNK then READLINK (`GATE_NFS readlink`). NFS SETATTR size (`GATE_NFS setattr`). NFS READDIR lists names (`GATE_NFS readdir`). NFS ACCESS grants file bits from mode (`GATE_NFS access`). NFS COMMIT persists an unstable WRITE (`GATE_NFS commit`). NFS GETFH after LOOKUP is a child handle (`GATE_NFS getfh`). NFS PUTFH restores it; unknown is STALE (`GATE_NFS putfh`). FAT32 / ext2 / ISO9660 ramdisk (`GATE_FAT32 block`, `GATE_EXT2 block`, `GATE_ISO9660 block`). FAT32 vfat / ISO9660 / NTFS / exFAT / Btrfs / XFS / ZFS mount (`GATE_FAT32 mount`, `GATE_ISO9660 mount`, `GATE_NTFS mount`, `GATE_EXFAT mount`, `GATE_BTRFS mount`, `GATE_XFS mount`, `GATE_ZFS mount`).
- [x] inotify — `emit_event` from VFS write/unlink and `open(O_CREAT)`; watch can read (**Gate H3**). Ring 3 `inotify_init1`/`add_watch`/`read` is live (Q3). fanotify MODIFY/CREATE on VFS write (`GATE_FANOTIFY modify`). Ring 3 `fanotify_init` returns ENOSYS (P4). io_uring NOP+WRITE (`GATE_IOURING sqe`); Ring 3 `io_uring_setup(8)` returns a ring fd (`GATE_CHR iouring`); `io_uring_setup(0,0)` stays ENOSYS (M4).
- [x] Quotas — `write_file_checked` returns `EDQUOT` when the hard limit is exceeded (`GATE_QUOTA enforce`)

### Perfect-OS next steps
1. ~~Make ext4 the root on VirtIO-blk by default.~~ **Done** (`GATE_C9 ext4 root`). Second virtio-blk is `mkfs.ext4` and mounted at `/`. Mount-point directories persist (C10); generated files under `/dev` `/proc` `/sys` `/run` `/tmp` stay virtual. Persist blob stays on vda (C1–C7, C10).
2. ~~Journal commit + crash recovery that is tested by killing QEMU mid-write.~~ **Done** for the persist blob WAL (C2) and JBD2 (C8: committed transaction applied, torn descriptor discarded).
3. ~~Complete VirtIO-blk DMA (guest-physical, not heap pointers).~~ **Done** — contiguous buddy frames + bounce buffer. ~~AHCI DMA still missing.~~ **C4 done.** ~~NVMe DMA still missing.~~ **C5 done.**
4. ~~Hook inotify on VFS mutate.~~ **Done** (H3). Page-cache writeback (C3) overlays dirty pages.

---
