#!/usr/bin/env python3
"""Write a minimal ext4 the KnoxOS driver can mount (no journal, 1 KiB blocks).

Used when host `mkfs.ext4` is missing. Superblock + one block group + root
inode 2 with `.` / `..` so `ext4::mkdir` / `create_file` can allocate.
"""
from __future__ import annotations

import struct
import sys

EXT4_MAGIC = 0xEF53
S_IFDIR = 0x4000
BLOCK = 1024
INODE_SIZE = 256
INODES = 256
FIRST_INO = 11


def u16(n: int) -> bytes:
    return struct.pack("<H", n)


def u32(n: int) -> bytes:
    return struct.pack("<I", n)


def write_superblock(img: bytearray, blocks: int, gdt_block: int) -> None:
    sb = bytearray(1024)
    # packed Ext4Superblock fields we actually read
    struct.pack_into("<I", sb, 0, INODES)  # s_inodes_count
    struct.pack_into("<I", sb, 4, blocks)  # s_blocks_count_lo
    struct.pack_into("<I", sb, 12, blocks - 70)  # s_free_blocks_count_lo
    struct.pack_into("<I", sb, 16, INODES - FIRST_INO + 1)  # s_free_inodes_count (approx)
    struct.pack_into("<I", sb, 20, 1)  # s_first_data_block
    struct.pack_into("<I", sb, 24, 0)  # s_log_block_size → 1024
    struct.pack_into("<I", sb, 32, blocks)  # s_blocks_per_group
    struct.pack_into("<I", sb, 40, INODES)  # s_inodes_per_group
    struct.pack_into("<H", sb, 56, EXT4_MAGIC)
    struct.pack_into("<H", sb, 58, 1)  # s_state clean
    struct.pack_into("<I", sb, 76, 1)  # s_rev_level
    struct.pack_into("<I", sb, 84, FIRST_INO)  # s_first_ino
    struct.pack_into("<H", sb, 88, INODE_SIZE)
    struct.pack_into("<H", sb, 104, 32)  # s_desc_size (unused unless 64-bit)
    name = b"knoxos-root"
    sb[120 : 120 + len(name)] = name
    img[1024:2048] = sb


def write_gdt(img: bytearray, free_blocks: int, free_inodes: int) -> None:
    gdt = bytearray(32)
    struct.pack_into("<I", gdt, 0, 3)  # bg_block_bitmap_lo
    struct.pack_into("<I", gdt, 4, 4)  # bg_inode_bitmap_lo
    struct.pack_into("<I", gdt, 8, 5)  # bg_inode_table_lo
    struct.pack_into("<H", gdt, 12, free_blocks & 0xFFFF)
    struct.pack_into("<H", gdt, 14, free_inodes & 0xFFFF)
    struct.pack_into("<H", gdt, 16, 1)  # bg_used_dirs_count_lo
    img[2 * BLOCK : 2 * BLOCK + 32] = gdt


def set_bits(bitmap: bytearray, used: int) -> None:
    for i in range(used):
        bitmap[i // 8] |= 1 << (i % 8)


def write_bitmaps(img: bytearray, used_blocks: int, used_inodes: int) -> None:
    bb = bytearray(BLOCK)
    ib = bytearray(BLOCK)
    set_bits(bb, used_blocks)
    set_bits(ib, used_inodes)
    img[3 * BLOCK : 4 * BLOCK] = bb
    img[4 * BLOCK : 5 * BLOCK] = ib


def write_root_inode(img: bytearray, dir_block: int) -> None:
    inode = bytearray(INODE_SIZE)
    struct.pack_into("<H", inode, 0, S_IFDIR | 0o755)
    struct.pack_into("<I", inode, 4, BLOCK)  # i_size_lo
    struct.pack_into("<H", inode, 26, 2)  # i_links_count
    struct.pack_into("<I", inode, 28, 2)  # i_blocks_lo (512-byte units)
    struct.pack_into("<I", inode, 40, dir_block)  # i_block[0]
    # inode 2 is index 1 in the table
    off = 5 * BLOCK + INODE_SIZE
    img[off : off + INODE_SIZE] = inode


def dirent(inode: int, name: bytes, ftype: int, rec_len: int) -> bytes:
    return struct.pack("<IHBB", inode, rec_len, len(name), ftype) + name + b"\0" * (
        rec_len - 8 - len(name)
    )


def write_root_dir(img: bytearray, dir_block: int) -> None:
    blk = bytearray(BLOCK)
    # . rec_len 12, .. takes the rest
    dot = dirent(2, b".", 2, 12)
    dotdot = dirent(2, b"..", 2, BLOCK - 12)
    blk[:12] = dot[:12]
    blk[12:] = dotdot
    img[dir_block * BLOCK : (dir_block + 1) * BLOCK] = blk


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <image>", file=sys.stderr)
        return 2
    path = sys.argv[1]
    with open(path, "r+b") as f:
        data = f.read()
    if len(data) < 64 * BLOCK:
        print("image too small", file=sys.stderr)
        return 1
    img = bytearray(data)
    blocks = len(img) // BLOCK
    used_blocks = 70
    used_inodes = FIRST_INO - 1  # 1..10 reserved; inode 2 is among them
    write_superblock(img, blocks, 2)
    write_gdt(img, blocks - used_blocks, INODES - used_inodes)
    write_bitmaps(img, used_blocks, used_inodes)
    write_root_inode(img, 69)
    write_root_dir(img, 69)
    with open(path, "r+b") as f:
        f.write(img)
    print(f"wrote minimal ext4 on {path} ({blocks} x {BLOCK}B blocks)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
