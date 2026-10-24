# Still open

← [Status index](README.md) · [Do this first](next.md)

Every remaining `- [ ]` in the subsystem files, plus unfinished Gate G. This is an inventory, not a sequence — start with [next.md](next.md).

## Gate G (quality bar)

| ID | Task | Done when |
|----|------|-----------|
| G3 | Feature flags: `gui`, `net`, `fs-ext4`, `stub-drivers` | **Done** — default `gui`+`net`+`fs-ext4`; `stub-drivers` opt-in (`GATE_G3 features live`) |
| G4 | Syscall audit spreadsheet: implemented / no-op / ENOSYS | **Done** — unused clock/time numbers ENOSYS (`GATE_G4 enosys`); `madvise`/`mlock*`/`readahead`/`fadvise64`/`set_robust_list` store real state (`GATE_G4 audit live`) |
| G5 | `unsafe` SAFETY comments + size budget | **Done** — 16 MB size gate; every `unsafe {` has a `// SAFETY:` comment; crate-level `undocumented_unsafe_blocks = deny` (`GATE_G5 safety comments`) |

## 1. Kernel Core

Details: [subsystems/01-kernel-core.md](subsystems/01-kernel-core.md)

- [x] **x86_64 / aarch64 / riscv64** — x86_64 desktop boots; aarch64/riscv64 QEMU serial `_start` (`GATE_ARCH aarch64 boot` / `GATE_ARCH riscv64 boot`). Full runtime on those arches is later.
- [x] **ACPI AML / DSDT** — DSDT mapped; `Name(_S5)` SLP_TYP extracted (`GATE_AML dsdt`). `acpi::shutdown` writes PM1a_CNT (`GATE_ACPI s5`).
- [x] **UEFI runtime** — `GetTime` from CMOS RTC; `SetVariable`/`GetVariable` round-trip (`GATE_UEFI time`)
- [x] **Kernel modules** — ELF ET_REL `.ko` load + reloc + `init_module` (`GATE_KO load`)
- [x] **IOMMU** — root table from buddy; `enable_translation` called (`GATE_IOMMU translation`)
- [x] **Live patch** — dedicated 4K trampoline, W^X (never W+X), activeness wait (`GATE_LPATCH wx`)
- [x] **Nested IRQ / TPR** — ISRs raise LAPIC TPR; nested enter writes TPR (`GATE_NIRQ tpr`)

## 2. Memory Management

Details: [subsystems/02-memory.md](subsystems/02-memory.md)

- [x] **NUMA** — `allocate_node` returns a buddy frame charged to the node (`GATE_NUMA frame`)
- [x] **KSM** — hashes via `phys_to_virt`; identical pages collapse and the victim is freed (`GATE_KSM merge`)
- [x] **THP** — buddy order-9 2 MiB allocation (`GATE_THP hugepage`)
- [x] **KASLR** for loadable kernel mappings (`GATE_KASLR slide`)
- [x] **Memory cgroups** enforcement — mmap/brk charge; hard `memory.max` is ENOMEM (`GATE_CGROUP memory`, `GATE_CGROUP2 memory`)

## 3. Process & Scheduling

Details: [subsystems/03-process.md](subsystems/03-process.md)

- [x] **sched_ext / eBPF** — verifier + interpreter `r0=42`; sched_ext pick (`GATE_EBPF exec`)

## 4. Filesystem & Storage

Details: [subsystems/04-filesystem.md](subsystems/04-filesystem.md)

- [x] NTFS — boot + MFT `$DATA` through the ramdisk block layer (`GATE_NTFS block`). `mount -t ntfs` reads `$DATA` (`GATE_NTFS mount`).
- [x] exFAT — VBR + FAT + directory `$DATA` through the ramdisk block layer (`GATE_EXFAT block`). `mount -t exfat` reads `$DATA` (`GATE_EXFAT mount`). CIFS SMB2 WRITE/READ (`GATE_CIFS smb2`). ZFS uberblock + Fletcher-4 (`GATE_ZFS block`). `mount -t zfs` reads `$DATA` (`GATE_ZFS mount`).
- [x] Btrfs — superblock + inode + extent `$DATA` through the ramdisk (`GATE_BTRFS block`). `mount -t btrfs` reads `$DATA` (`GATE_BTRFS mount`).
- [x] XFS — superblock + inode + extent `$DATA` through the ramdisk (`GATE_XFS block`). `mount -t xfs` reads `$DATA` (`GATE_XFS mount`).
- [x] JBD2 journal replay — descriptor + commit applied; torn descriptor discarded (`GATE_C8 jbd2 replay`). Virtual-FS mount-point directories persist (`GATE_C10 virtfs persist`) including `/tmp`. FAT32 / ext2 / ISO9660 through the ramdisk (`GATE_FAT32 block`, `GATE_EXT2 block`, `GATE_ISO9660 block`). FAT32 `mount -t vfat` reads `$DATA` (`GATE_FAT32 mount`). ISO9660 `mount -t iso9660` reads `$DATA` (`GATE_ISO9660 mount`).
- [x] FUSE — LOOKUP + READ through the in-kernel loopback daemon (`GATE_FUSE lookup`). WRITE stores bytes a later READ returns (`GATE_FUSE write`). UNLINK drops the name (`GATE_FUSE unlink`). MKDIR LOOKUP returns `S_IFDIR` (`GATE_FUSE mkdir`). RMDIR drops the directory (`GATE_FUSE rmdir`). RENAME moves the name (`GATE_FUSE rename`). SYMLINK LOOKUP returns `S_IFLNK` and READLINK returns the target (`GATE_FUSE symlink`). LINK LOOKUP finds the same inode (`GATE_FUSE link`). CREATE LOOKUP returns `S_IFREG` (`GATE_FUSE create`). GETATTR returns size and `S_IFREG` (`GATE_FUSE getattr`). SETATTR changes size (`GATE_FUSE setattr`). READDIR lists created names (`GATE_FUSE readdir`). MKNOD LOOKUP returns `S_IFCHR` (`GATE_FUSE mknod`). STATFS reports `bsize` 4096 (`GATE_FUSE statfs`). NFS4 COMPOUND PUTROOTFH+LOOKUP+READ (`GATE_NFS compound`). WRITE stores bytes a later READ returns (`GATE_NFS write`). REMOVE then LOOKUP is `NFS4ERR_NOENT` (`GATE_NFS remove`). RENAME then LOOKUP finds the new name (`GATE_NFS rename`). CREATE then LOOKUP (`GATE_NFS create`). LINK then LOOKUP finds both names (`GATE_NFS link`). CREATE NF4LNK then READLINK (`GATE_NFS readlink`). SETATTR changes size (`GATE_NFS setattr`). READDIR lists created names (`GATE_NFS readdir`). SETXATTR then GETXATTR (`GATE_FUSE setxattr`). GETXATTR missing name fails (`GATE_FUSE getxattr`). LISTXATTR includes the stored name (`GATE_FUSE listxattr`). REMOVEXATTR drops it (`GATE_FUSE removexattr`). ACCESS grants file bits from mode (`GATE_NFS access`). COMMIT persists an unstable WRITE (`GATE_NFS commit`). GETFH after LOOKUP is a child handle (`GATE_NFS getfh`). PUTFH restores it; unknown is STALE (`GATE_NFS putfh`). ACCESS grants file bits from mode (`GATE_FUSE access`). FSYNC after WRITE (`GATE_FUSE fsync`). OPEN returns fh=nodeid (`GATE_FUSE open`). RELEASE after OPEN (`GATE_FUSE release`). LOOKUPP of a child is the export root (`GATE_NFS lookupp`). RESTOREFH restores SAVEFH; missing is NOFILEHANDLE (`GATE_NFS restorefh`). PUTPUBFH after LOOKUP is the export root (`GATE_NFS putpubfh`). VERIFY matching size is OK; mismatch is NOT_SAME (`GATE_NFS verify`). FLUSH after WRITE (`GATE_FUSE flush`). FSYNCDIR of a directory (`GATE_FUSE fsyncdir`). NVERIFY mismatch is OK; match is SAME (`GATE_NFS nverify`). SETCLIENTID_CONFIRM of the issued client_id (`GATE_NFS confirm`). OPENDIR returns fh=nodeid (`GATE_FUSE opendir`). RELEASEDIR after OPENDIR (`GATE_FUSE releasedir`). GETLK of a known file with no lock is F_UNLCK (`GATE_FUSE getlk`). SETLK F_WRLCK is visible to GETLK from another pid (`GATE_FUSE setlk`). CLOSE of the issued OPEN (`GATE_NFS close`). RENEW of a confirmed client_id (`GATE_NFS renew`). LOCK of a free range is OK; overlap is DENIED (`GATE_NFS lock`). LOCKU of a held range is OK; missing is DENIED (`GATE_NFS unlock`). SETLKW F_WRLCK is visible to GETLK (`GATE_FUSE setlkw`). BMAP of a known file maps block 0 to nodeid (`GATE_FUSE bmap`). DESTROY of a known mount succeeds (`GATE_FUSE destroy`). IOCTL of a known file echoes cmd (`GATE_FUSE ioctl`). POLL of a known file with data is POLLIN (`GATE_FUSE poll`). FALLOCATE extends size (`GATE_FUSE fallocate`). LOCKT of a free range is OK; overlap is DENIED (`GATE_NFS lockt`). RELEASE_LOCKOWNER drops the owner's locks (`GATE_NFS lockowner`). OPEN_CONFIRM of the issued OPEN (`GATE_NFS openconfirm`). OPEN_DOWNGRADE to a subset (`GATE_NFS downgrade`). SECINFO of a known name is AUTH_SYS (`GATE_NFS secinfo`). OPENATTR createdir (`GATE_NFS openattr`). READDIRPLUS lists names with size (`GATE_FUSE readdirplus`). RENAME2 moves then NOREPLACE fails (`GATE_FUSE rename2`). LSEEK SEEK_SET 0 then SEEK_END (`GATE_FUSE lseek`). COPY_FILE_RANGE copies bytes (`GATE_FUSE copy`). SYNCFS of a known mount (`GATE_FUSE syncfs`). STATX size and `S_IFREG` (`GATE_FUSE statx`). DELEGPURGE of a confirmed client_id (`GATE_NFS delegpurge`). DELEGRETURN of an OPEN state_id (`GATE_NFS delegreturn`). EXCHANGE_ID of a client_name (`GATE_NFS exchangeid`). CREATE_SESSION of that client_id (`GATE_NFS session`). DESTROY_SESSION of that session (`GATE_NFS destroysession`). DESTROY_CLIENTID after DESTROY_SESSION (`GATE_NFS destroyclientid`). CPIO newc (`GATE_CPIO newc`). dpkg ar+tar (`GATE_DPKG deb`).
- [x] Quotas — `write_file_checked` returns `EDQUOT` (`GATE_QUOTA enforce`). fanotify MODIFY/CREATE on VFS write (`GATE_FANOTIFY modify`); Ring 3 `fanotify_init` stays ENOSYS. io_uring NOP+WRITE (`GATE_IOURING sqe`); Ring 3 `io_uring_setup` stays ENOSYS.

## 5. Networking Stack

Details: [subsystems/05-networking.md](subsystems/05-networking.md)

- [x] IPv6 echo — loopback `::1` Echo Request produces a TX reply (`GATE_D6 ipv6 echo`)
- [x] TLS 1.3 HKDF — RFC 5869 extract/expand + Expand-Label (`GATE_TLS13 hkdf`). ClientHello+ServerHello matching traffic secret (`GATE_TLS13 hs`). DER + RSA PKCS#1 over TBS (`GATE_TLS13 x509`). NFS4 WRITE then READ (`GATE_NFS write`). NFS4 REMOVE then LOOKUP `NFS4ERR_NOENT` (`GATE_NFS remove`). NFS4 RENAME then LOOKUP (`GATE_NFS rename`). NFS4 CREATE then LOOKUP (`GATE_NFS create`). NFS4 LINK then LOOKUP (`GATE_NFS link`). NFS4 CREATE NF4LNK then READLINK (`GATE_NFS readlink`). NFS4 SETATTR then GETATTR/READ (`GATE_NFS setattr`). NFS4 READDIR lists names (`GATE_NFS readdir`). NFS4 ACCESS grants file bits from mode (`GATE_NFS access`). NFS4 COMMIT persists an unstable WRITE (`GATE_NFS commit`). NFS4 GETFH after LOOKUP is a child handle (`GATE_NFS getfh`). NFS4 PUTFH restores it; unknown is STALE (`GATE_NFS putfh`). NFS4 LOOKUPP of a child is the export root (`GATE_NFS lookupp`). NFS4 RESTOREFH restores SAVEFH; missing is NOFILEHANDLE (`GATE_NFS restorefh`). NFS4 PUTPUBFH after LOOKUP is the export root (`GATE_NFS putpubfh`). NFS4 VERIFY matching size is OK; mismatch is NOT_SAME (`GATE_NFS verify`). NFS4 NVERIFY mismatch is OK; match is SAME (`GATE_NFS nverify`). NFS4 SETCLIENTID_CONFIRM of the issued client_id (`GATE_NFS confirm`). NFS4 CLOSE of the issued OPEN (`GATE_NFS close`). NFS4 RENEW of a confirmed client_id (`GATE_NFS renew`). NFS4 LOCK of a free range is OK; overlap is DENIED (`GATE_NFS lock`). NFS4 LOCKU of a held range is OK; missing is DENIED (`GATE_NFS unlock`). NFS4 LOCKT of a free range is OK; overlap is DENIED (`GATE_NFS lockt`). NFS4 RELEASE_LOCKOWNER drops the owner's locks (`GATE_NFS lockowner`). NFS4 OPEN_CONFIRM of the issued OPEN (`GATE_NFS openconfirm`). NFS4 OPEN_DOWNGRADE to a subset (`GATE_NFS downgrade`). NFS4 SECINFO of a known name is AUTH_SYS (`GATE_NFS secinfo`). NFS4 OPENATTR createdir (`GATE_NFS openattr`). NFS4 DELEGPURGE of a confirmed client_id (`GATE_NFS delegpurge`). NFS4 DELEGRETURN of an OPEN state_id (`GATE_NFS delegreturn`). NFS4 EXCHANGE_ID of a client_name (`GATE_NFS exchangeid`). NFS4 CREATE_SESSION of that client_id (`GATE_NFS session`). NFS4 DESTROY_SESSION of that session (`GATE_NFS destroysession`). NFS4 DESTROY_CLIENTID after DESTROY_SESSION (`GATE_NFS destroyclientid`).
- [x] NAT — IPv4 SNAT/DNAT rewrite with header checksum (`GATE_NAT masquerade`)
- [x] VLAN — 802.1Q insert/strip (`GATE_VLAN tag`). Wi-Fi Beacon + WPA2 PMK (`GATE_WIFI pmk`). WPA3-SAE matching PMK (`GATE_WPA3 sae`); bridge FDB (`GATE_BRIDGE fdb`).

## 6. Device Drivers

Details: [subsystems/06-drivers.md](subsystems/06-drivers.md)

- [x] XHCI BAR0 mapped uncached + buddy DMA rings (`GATE_USB xhci mmio`). Enable Slot Command Completion cc=1 (`GATE_USB xhci slot`). Address Device Command Completion cc=1 (`GATE_USB xhci addr`). GET_DESCRIPTOR Transfer Event (`GATE_USB xhci desc`). SET_CONFIGURATION Transfer Event (`GATE_USB xhci config`). Configure Endpoint Command Completion cc=1 (`GATE_USB xhci ep`). GET_DESCRIPTOR(Configuration) Transfer Event plus full enum with no timeout fallback (`GATE_USB xhci enum`). GET_STATUS Transfer Event (`GATE_USB xhci status`). SET_INTERFACE Transfer Event (`GATE_USB xhci iface`). GET_CONFIGURATION Transfer Event (`GATE_USB xhci getcfg`). GET_INTERFACE Transfer Event (`GATE_USB xhci getiface`). CLEAR_FEATURE Transfer Event (`GATE_USB xhci clear`). bulk OUT Normal TRB Transfer Event (`GATE_USB xhci bulk`). SET_FEATURE Transfer Event (`GATE_USB xhci set`). bulk IN Normal TRB Transfer Event (`GATE_USB xhci bulkin`). SET_DESCRIPTOR Transfer Event (`GATE_USB xhci stdesc`). isochronous IN Isoch TRB Transfer Event (`GATE_USB xhci isoch`). GET_DESCRIPTOR(String) Transfer Event (`GATE_USB xhci string`). interrupt IN Normal TRB Transfer Event (`GATE_USB xhci intr`). No-Op Command Completion cc=1 (`GATE_USB xhci noop`). Disable Slot Command Completion cc=1 (`GATE_USB xhci disable`). Evaluate Context Command Completion cc=1 (`GATE_USB xhci eval`). Stop Endpoint bulk OUT (DCI 2) Command Completion cc=1 (`GATE_USB xhci stop`). Reset Endpoint Halted bulk IN (DCI 5) Command Completion cc=1 (`GATE_USB xhci resetep`). Set TR Dequeue Pointer Stopped extra pipe Command Completion cc=1 (`GATE_USB xhci settr`). Reset Device already-addressed slot Command Completion cc=1 (`GATE_USB xhci resetdev`). Transfer No-Op bulk OUT (DCI 2) Transfer Event (`GATE_USB xhci trnoop`). GET_DESCRIPTOR(Device Qualifier) Transfer Event (`GATE_USB xhci qual`). GET_DESCRIPTOR(BOS) Transfer Event (`GATE_USB xhci bos`). GET_DESCRIPTOR(Other Speed Configuration) Transfer Event (`GATE_USB xhci oscfg`). SYNCH_FRAME Transfer Event (`GATE_USB xhci synch`). SET_SEL Transfer Event (`GATE_USB xhci setsel`). SET_ISOCH_DELAY Transfer Event (`GATE_USB xhci isochdelay`). GET_DESCRIPTOR(HID) Transfer Event (`GATE_USB xhci hiddesc`). GET_DESCRIPTOR(HID Report) Transfer Event (`GATE_USB xhci hidreport`). GET_REPORT Transfer Event (`GATE_USB xhci getreport`). SET_REPORT Transfer Event (`GATE_USB xhci setreport`). GET_IDLE Transfer Event (`GATE_USB xhci getidle`). SET_IDLE Transfer Event (`GATE_USB xhci setidle`). GET_PROTOCOL Transfer Event (`GATE_USB xhci getprotocol`). SET_PROTOCOL Transfer Event (`GATE_USB xhci setprotocol`). Transfer Events drain into HID.
- [x] i915 modeset — pipe A + plane + GTT[0] in shadow MMIO (`GATE_I915 modeset`). AMDGPU firmware (`GATE_AMDGPU fw`).
- [x] HDA — CORB/RIRB + PCM BDL from buddy frames (`GATE_HDA dma`)
- [x] TPM TIS FIFO — locality 0 REQUEST_USE grants VALID|ACTIVE (`GATE_TPM tis`). Software SHA-256 PCR extend (`GATE_TPM pcr`). Hardware 0xFED4_0000 still unmapped on stock QEMU.
- [x] VirtIO-GPU — controlq + cursorq vs QEMU `-vga virtio` (`GATE_F8 virtio-gpu scanout`, `GATE_F8 hw cursor`, `GATE_GPU modern stack`, `GATE_GPU pointer plane`)
- [x] DRM — GEM from buddy frames + CRTC set (`GATE_DRM gem`). HDR PQ + VRR Adaptive-Sync (`GATE_HDR pq`, `GATE_VRR range`).
- [x] USB HID URB completion — submit + complete + boot-protocol parse (`GATE_USB hid urb`). Transfer Events feed the same handler and the GUI mouse/key path.
- [x] USB MSC — SCSI READ(10) vs ramdisk LUN (`GATE_USB msc`). USB Audio SET_CUR + PCM loopback (`GATE_USB audio`).
- [x] USB hub — descriptor type 0x29 + GetPortStatus (`GATE_USB hub`)
- [x] USB HID gamepad — 8-byte report parse (`GATE_USB gamepad`)
- [x] I2C SMBus EEPROM write/read (`GATE_I2C xfer`)
- [x] e1000 — `Socket::send` programs the TX ring; loopback RX (`GATE_E1000 send`). rtl8139 TX→RX (`GATE_RTL8139 send`).
- [x] Touchscreen HID digitizer (`GATE_USB touch`); trackpad two-finger scroll (`GATE_USB trackpad`); GPIO pin loopback (`GATE_GPIO xfer`); Wacom pen (`GATE_USB wacom`); UVC YUY2 (`GATE_USB uvc`)
- [x] USB Ethernet CDC ECM frame loopback (`GATE_USB ethernet`). AX211 iwlwifi TLV (`GATE_AX211 fw`). Broadcom `BRCMFW` (`GATE_BRCM fw`). Thunderbolt route hop (`GATE_TB hop`).

## 7. GUI & Desktop Environment

Details: [subsystems/07-gui.md](subsystems/07-gui.md)

- [x] **Wayland protocol** — `/dev/wl0` ioctl (F1–F2) plus AF_UNIX `/run/wayland-0` (F5) plus `wl_display` object opcodes (`GATE_WL display protocol`)
- [x] **GPU compositor** — `gpu_compositor` init + VSync; desktop `present_rect` flushes through VirtIO-GPU (`GATE_GPU desktop scanout`). `-vga virtio` is the window (`GATE_GPU modern stack`).
- [x] Hardware cursor — VirtIO-GPU cursorq `UPDATE_CURSOR` / `MOVE_CURSOR` is the pointing device on the primary scanout (`GATE_F8 hw cursor`, `GATE_GPU pointer plane`).
- [x] VSync — `gpu_compositor` software VBlank flip on the live path (`GATE_VSYNC paced`); desktop still software-blits to VGA
- [x] IME — pinyin `"ni"` → `你` (`GATE_IME compose`). Full IME framework later.
- [x] TrueType bytecode hinting — SVTCA/PUSHB/MDAP round-to-grid (`GATE_TTF hint`). Title `'i'` stem (`GATE_FONT title`).
- [x] HDR, VRR — PQ EOTF + Adaptive-Sync range (`GATE_HDR pq`, `GATE_VRR range`)
- [x] Ring 3 `openpty` + SHM terminal client (`GATE_TERM pty userspace`). Start-menu Terminal is Ring 3; master fd + slave I/O (`GATE_TERM pty interactive`). First SHM frame paints glyphs (`GATE_TERM shm paint`).

## 8. Shell & Terminal

Details: [subsystems/08-shell.md](subsystems/08-shell.md)

- [x] Full VT100/xterm-256 — CUP + SGR 256-color + ED (`GATE_VT100 xterm`)
- [x] Here-docs, functions, `~/.profile` (`GATE_HEREDOC expand`, `GATE_PROFILE sourced`)
- [x] Job control against **processes** — SIGTSTP stops; `waitpid(WUNTRACED)` reports; SIGCONT resumes (`GATE_JOB stop`). Ring 3 `pipe()` is live (M2).

## 9. Security & Cryptography

Details: [subsystems/09-security.md](subsystems/09-security.md)

- [x] AES-GCM, Poly1305 — NIST AES-128-GCM + RFC 8439 Poly1305 (`GATE_AESGCM kat`, `GATE_POLY1305 kat`). TLS 1.3 HKDF (`GATE_TLS13 hkdf`) plus ClientHello/ServerHello (`GATE_TLS13 hs`). DER + RSA PKCS#1 over TBS (`GATE_TLS13 x509`).
- [x] X25519 / Ed25519 — RFC 7748 Alice public + RFC 8032 TEST 1 (`GATE_X25519 kat`, `GATE_ED25519 kat`)
- [x] RSA / ECDSA-P256 — PKCS#1 v1.5 SHA-256 + RFC 6979 P-256 (`GATE_RSA kat`, `GATE_ECDSA kat`)
- [x] Kernel lockdown, secure wipe on free, IMA — unsigned `insert_module` denied (`GATE_LOCKDOWN module`); buddy free zeros the frame (`GATE_WIPE free`); exec measures SHA-256 (`GATE_IMA exec`)
- [x] SELinux — Enforcing TE on VFS writes (`GATE_SELINUX deny`). Default stays Permissive. AppArmor path MAC on VFS writes (`GATE_APPARMOR deny`).
- [x] dm-verity — SHA-256 Merkle leaf equals root; tamper rejected (`GATE_VERITY root`)

## 10. System Services

Details: [subsystems/10-services.md](subsystems/10-services.md)

- [x] Crash restart of failed units (`GATE_SVC restart`). Unit `MemoryMax=` cgroup (`GATE_SVC cgroup`). D-Bus `Hello` unique name (`GATE_DBUS method`). `RequestName` PRIMARY_OWNER (`GATE_DBUS name`). `ReleaseName` drops ownership (`GATE_DBUS release`). `ListNames` includes the well-known name (`GATE_DBUS list`). `GetConnectionUnixProcessID` returns the pid (`GATE_DBUS pid`). `GetNameOwner` returns the unique name (`GATE_DBUS owner`). `NameHasOwner` is false then true (`GATE_DBUS hasowner`). `GetConnectionUnixUser` returns the uid (`GATE_DBUS uid`). `AddMatch` delivers matching signals (`GATE_DBUS match`). `RemoveMatch` stops delivery (`GATE_DBUS remove`). `GetId` returns the machine UUID (`GATE_DBUS id`). `StartServiceByName` SUCCESS then ALREADY_RUNNING (`GATE_DBUS start`). `GetConnectionCredentials` returns pid and uid (`GATE_DBUS creds`). `ListActivatableNames` includes the seeded name (`GATE_DBUS activate`). `ListQueuedOwners` returns a queued unique name (`GATE_DBUS queue`). `GetConnectionSELinuxSecurityContext` returns the context (`GATE_DBUS selinux`). `GetConnectionAppArmorSecurityContext` returns `unconfined` (`GATE_DBUS apparmor`). `GetAdtAuditSessionData` returns the session bytes (`GATE_DBUS adt`). `UpdateActivationEnvironment` returns a method reply (`GATE_DBUS env`). `ReloadConfig` returns a method reply (`GATE_DBUS reload`). `BecomeMonitor` then signals without AddMatch (`GATE_DBUS monitor`). `Peer.Ping` method return (`GATE_DBUS ping`). `Peer.GetMachineId` 32-hex UUID (`GATE_DBUS machineid`). `Introspectable.Introspect` XML (`GATE_DBUS introspect`). `Properties.Get` Features then UnknownProperty (`GATE_DBUS props`). `Properties.GetAll` Features then UnknownInterface (`GATE_DBUS getall`). `Properties.Set` Features PropertyReadOnly then UnknownProperty (`GATE_DBUS setprop`). `Debug.Stats.GetStats` Connections (`GATE_DBUS stats`). `GetAllMatchRules` after AddMatch (`GATE_DBUS matchrules`). `GetConnectionStats` UnixProcessID (`GATE_DBUS connstats`). `GetManagedObjects` `/org/freedesktop/DBus` (`GATE_DBUS objects`). `NameOwnerChanged` after RequestName (`GATE_DBUS ownerchanged`). `NameLost` after ReleaseName (`GATE_DBUS namelost`). `NameAcquired` after RequestName (`GATE_DBUS nameacquired`). `GetConnectionLinuxSecurityLabel` returns the LSM label (`GATE_DBUS label`). `GetConfigFile` returns the system bus path (`GATE_DBUS config`). `ActivatableServicesChanged` after ReloadConfig (`GATE_DBUS svcchanged`). PAM unix SHA-512 (`GATE_PAM unix`).
- [x] NTP — fixture Server-mode packet applies a 1000 ms offset (`GATE_NTP apply`)
- [x] Bluetooth HCI — virtual adapter Reset Command Complete (`GATE_BT hci`)
- [x] Cron `@reboot` plus PIT/APIC due-flag (`GATE_CRON reboot`)
- [x] udev-style hotplug → `/dev` from PCI scan (`GATE_UDEV hotplug`)
- [x] NetworkManager activate copies live `eth0` DHCP (`GATE_NM dhcp`)
- [x] Suspend/hibernate, logind — S3 save/restore hooks (`GATE_SUSPEND resume`); session create/list/terminate (`GATE_LOGIND session`)
- [x] PulseAudio mixing to HDA (`GATE_PA mix`)

## 11. Virtualization & Containers

Details: [subsystems/11-virt.md](subsystems/11-virt.md)

- [x] `kvm::start_vm` software EPT fetch of HLT `0xF4` (`GATE_KVM vmlaunch`). Nested VMXON is not used on stock QEMU.
- [x] EPT 4-level walk + virtio-mmio magic `0x74726976` (`GATE_KVM ept`)
- [x] `container.rs` — `runc start` unshare + `pivot_root` (`GATE_OCI start`)
- [x] cgroup v2 **memory** enforcement (`GATE_CGROUP2 memory`). CPU quota and IO byte cap deny (`GATE_CGROUP cpu`, `GATE_CGROUP io`). Frozen cgroup denies CPU charge (`GATE_CGROUP freeze`).

## 12. AI/ML Integration

Details: [subsystems/12-ai.md](subsystems/12-ai.md)

- [x] Scaled-dot-product attention + KV cache append (`GATE_AI attn`). BPE encode/decode of `"the"` (`GATE_AI vocab`). Production tokenizer vocab later.
- [x] `gpu_matmul.rs` CPU KAT with `available: true` (`GATE_GPU matmul`)
- [x] GUI assistant summarize uses attention, not placeholder text (`GATE_AI chat`)

## 13. Binary Compatibility & Runtime

Details: [subsystems/13-binary-compat.md](subsystems/13-binary-compat.md)

- [x] Dynamic linker as a userspace `ld.so` (DT_NEEDED, lazy PLT, RELRO) — **Gate BM1**
- [x] `dlopen` that maps `.so` via VMM (`open` + file-backed `mmap`, not `ldknoxos.rs` fake base) — **Gate BM2**
- [x] Static musl CRT ABI (`_start` → `__libc_start_main` → `main`) — **Gate BM3**
- [x] Dynamically linked musl binary for Ring 3 (`PT_INTERP` + `DT_NEEDED libc.so` + `__libc_start_main@plt`) — **Gate BM4**
- [x] musl CRT client calls `KnoxUiRaster` (`GATE_GPUI musl raster`)
- [x] Interactive Ring 3 calc re-rasters SHM on key (`GATE_GPUI ring3 calc key`)
- [x] Ring 3 `/bin/sh` parser via `ShellExecute` (`GATE_SH ring3 parser`)
- [x] D-Bus daemon as a user process (`GATE_DBUS daemon userspace`)
- [x] Wayland listen owned by a user process (`GATE_WL server userspace`)

## 14. Internationalization & Fonts

Details: [subsystems/14-i18n.md](subsystems/14-i18n.md)

- [x] Font fallback chain — compositor chain used for CJK/emoji (`GATE_FONT fallback`)
- [x] GSUB Latin ligatures — `shape_text("fi")` → U+FB01 (`GATE_GSUB liga`). GPOS A+V kern (`GATE_GPOS kern`). GPOS mark-to-base `e`+U+0301 (`GATE_GPOS mark`).
- [x] Color emoji — U+1F600 multi-color bitmap (`GATE_EMOJI color`)
- [x] Locale `.mo` — GNU magic load + `set_locale` + `gettext("File")=="Archivo"` (`GATE_LOCALE switch`)

## 15. Build System & Tooling

Details: [subsystems/15-build.md](subsystems/15-build.md)

- [x] **`flake.nix`** — present (devShell + QEMU/Make; kernel still rustup nightly)
- [x] Cargo workspace unifying `kernel` + `boot` + `tools` (`default-members` are host tools; kernel via `./scripts/kernel-cargo.sh`)
- [x] Feature flags so Unused modules do not compile into the demo kernel (`gui`/`net` default; `stub-drivers`/`fs-ext4` opt-in)
- [x] ISO + SHA256SUMS + optional GPG (`scripts/make-iso.sh`, `scripts/release-checksums.sh`). PK/KEK/db + OVMF enroll via `scripts/secure-boot-enroll.sh`. QEMU Secure Boot boot: `scripts/qemu-secure-boot.sh` (`GATE_SB ovmf boot`).

## 16. Testing & Quality

Details: [subsystems/16-testing.md](subsystems/16-testing.md)

- [x] Host-side `#[test]` in `tools/host-tests` (`cargo test -p knoxos-host-tests`)
- [x] Clippy `undocumented_unsafe_blocks` is **deny** (`kernel/Cargo.toml`); every `unsafe {` has a `// SAFETY:` comment (`GATE_G5 safety comments`).
- [x] Host parser tests (ELF / ICMPv6 / cron / ext4 magic / quota KiB / Btrfs magic / VLAN TPID / Beacon SSID)
- [x] Host fuzz of ELF / ext4 / packet parsers (`cargo test -p knoxos-host-tests`; malformed inputs must not panic). No Miri, no coverage.

## 17. Documentation

Details: [subsystems/17-docs.md](subsystems/17-docs.md)

- [x] Architecture / syscall / driver / GUI guides (`docs/`)
- [x] `cargo doc` published (CI rustdoc artifact)
- [x] Changelog (`CHANGELOG.md`)

## 18. CI/CD & Release

Details: [subsystems/18-ci.md](subsystems/18-ci.md)

- [x] Cross-compile aarch64/riscv64 on every PR (compile only; QEMU boot of those arches is later)
- [x] Cross-arch QEMU **boot** (`scripts/qemu-cross-boot.sh`: `GATE_ARCH aarch64 boot` / `GATE_ARCH riscv64 boot`)
- [x] `cargo-audit`, license/SPDX check (CI + `scripts/license-check.sh`)
- [x] Tagged release → ISO → checksums → signatures (`scripts/make-iso.sh` + `release-checksums.sh`; GPG optional). PK/KEK/db + OVMF Secure Boot QEMU boot (`scripts/qemu-secure-boot.sh`).
- [x] Nightly main-branch images (`.github/workflows/nightly.yml`)
