# What actually works today

← [Status index](README.md) · [Do this first](next.md)

KnoxOS **does boot in QEMU** to an in-kernel software desktop. This is real and worth keeping.

### Live path (QEMU)

1. Bootloader maps physical memory and a 1920×1080 framebuffer.
2. GDT + TSS, IDT, PIC 8259, LAPIC timer, serial UART TX.
3. 512 MiB kernel heap (`linked_list_allocator` + slab).
4. Desktop pointer: VirtIO-tablet absolute (`GATE_VIRTIO input tablet`), USB HID digitizer, PS/2 aux fallback. `GATE_USB no ps2 cursor` is a USB URB KAT, not “skip i8042”.
5. In-memory VFS with Linux FHS layout; **opt-in** VirtIO-blk + ATA PIO; ext4 and FAT32 can use that block layer. **Gate C1** persist blob store round-trips a file through VirtIO-blk (`GATE_C1 persist complete`). **Gate C2** write-ahead log replays a committed record after a simulated crash (`GATE_C2 journal recovered`). **Gate C3** dirty page writeback overlays a 4 KiB range without replacing sibling pages (`GATE_C3 writeback complete`). **Gate C4** AHCI command-list DMA write/read round-trips a sector vs QEMU (`GATE_C4 ahci dma complete`). **Gate C5** NVMe PRP DMA write/read round-trips a sector (`GATE_C5 nvme dma complete`). **Gate C6** `/etc` (outside the old persist prefixes) round-trips through VirtIO-blk (`GATE_C6 vfs persist`).
6. Software compositor: 32bpp BGRA, damage rects, window manager, taskbar, start menu, 17 in-process apps.
7. Kernel shell + terminal (parser, pipes, glob, env, 60+ builtins) running **inside the kernel**, not as `/bin/sh` in Ring 3.
8. **Gate B2 hello** — static ELF `iretq`s to Ring 3, `sys_write`s `hello from userspace`, `sys_exit`s back to the kernel.
9. **Gate B3–B6 scheduled Ring 3** — `execve(/bin/hello)` + `waitpid`; `fork` child runs and is reaped; SIGKILL / SIGSEGV / PTY Ctrl+C terminate user tasks; `/bin/sh` runs on a kernel PTY.
10. **Gate D1 loopback sockets** — UDP/TCP `send` copies into the peer `recv_buf` on `lo`; a Ring 3 program `sendto`/`recvfrom`s `ping` and prints `GATE_D1 loopback complete`.
11. **Gate D2 virtio-net** — TX writes the avail ring with guest-physical bounce buffers; RX walks the used ring; a DHCP DISCOVER gets a UDP reply from QEMU user-net (`GATE_D2 virtio-net complete`).
12. **Gate D3 DHCP apply** — DISCOVER/OFFER/REQUEST/ACK writes `eth0` IPv4 + default route (`GATE_D3 dhcp applied`).
13. **Gate D4 DNS + TCP** — UDP DNS to QEMU user-net; TCP SYN/ACK plus RTO retransmit; HTTP GET to `10.0.2.100` (`GATE_D4 dns tcp complete`).
14. **Gate D5 TCP CUBIC** — congestion window grows on ACK, shrinks on loss, and limits send (`GATE_D5 cubic window`).
15. **Gate E1–E4 enforcement** — W^X/`mprotect` RWX denied + ASLR; ChaCha20 `getrandom`; seccomp EPERM; unprivileged bind `<1024` fails.
16. **Gate B7 sigreturn** — Ring 3 `rt_sigaction(SIGINT)` handler `ret`s into a trampoline; `rt_sigreturn` restores `pause` (`GATE_B7 sigreturn complete`).
17. **Gate B8 timer preempt** — a Ring 3 `jmp $` spinner is switched out by the APIC timer without a syscall; a peer writer runs (`GATE_B8 timer preempt complete`).
18. **Gate F1–F5 isolated clients** — Ring 3 program mmaps a 64×64 BGRA buffer, `ioctl(/dev/wl0)` presents it; compositor SHM round-trips the pixels (`GATE_F1 client isolated`, `GATE_F2 shm commit`). A Ring 3 terminal client presents as Empty+SHM, not `WindowContentType::Terminal` (`GATE_F3 terminal isolated`). Empty launcher stubs are gone; Paint is a Ring 3 SHM client (`GATE_F4 launcher userspace`). Compositor `bind`+`listen`s `/run/wayland-0`; a Ring 3 client `connect`s and `sendto`s the SHM vaddr (`GATE_F5 wayland unix`).
19. **Gate H1–H4** — CoW write-fault copies (H1); file-backed mmap faults one page (H2); inotify sees VFS write/unlink (H3); guarded kernel stack + OOM from empty buddy pool (H4).
20. Async executor loop: keyboard, mouse, ~60 FPS redraw. Idle kernel thread `HLT`s when the desktop has no work.
21. **Gate I1 SMP online** — per-CPU TSS + GS-relative `CpuLocal`; AP INIT/SIPI reaches 64-bit and loads its own TSS (`GATE_I1 smp online`).
22. **Gate I2 IRQ GPRs** — naked APIC-timer stub saves all GPRs + FXSAVE; spinner `rbx` magic survives preemption (`GATE_I2 irq gprs`).
23. **Gate I3 AP Ring 3** — a `getcpu` ELF affinity-pinned to CPU 1 `iretq`s on the AP, syscalls, and exits (`GATE_I3 ap ring3`).
24. **Gate BM1 userspace `ld.so`** — a Ring 3 ELF with `PT_INTERP` `/lib/ld-knoxos.so` and `DT_NEEDED libgate_bm.so` runs: the interpreter maps the `.so`, applies GNU_RELRO, and the first PLT call resolves `gate_hello` (`GATE_BM1 dynlink complete`).
25. **Gate BM2 real `dlopen`** — a static Ring 3 program `open`s `/lib/libgate_bm.so`, `mmap`s it through the VMM, walks dynsym for `gate_hello`, calls it (`0x42`), and writes the marker (`GATE_BM2 dlopen complete`).
26. **Gate BM3 static musl CRT** — a Ring 3 binary that is not the hello fixture: musl `crt1` `_start` calls `__libc_start_main`, which calls `main` (`GATE_BM3 musl abi`).
27. **Gate BM4 dynamically linked musl** — PT_INTERP `/lib/ld-knoxos.so`, DT_NEEDED `/lib/libc.so`; crt1 `_start` calls `__libc_start_main@plt`, which calls `main` (`GATE_BM4 musl dynlink`). Generated in-kernel (same class as BM1–BM3), not a host musl-gcc binary.
28. **Gate C7 `/bin` persist** — a file under `/bin` round-trips through VirtIO-blk (`GATE_C7 bin persist`).
29. **Gate G3 feature flags** — default kernel is `gui`+`net`; `stub-drivers` and `fs-ext4` are off (`GATE_G3 features live`).
30. **Gate G4 honesty** — `adjtimex` returns `-ENOSYS` from Ring 3 (`GATE_G4 enosys`); `sched_setaffinity` writes a real mask.
31. **Gate F6 calculator userspace** — Ring 3 SHM client, not `WindowContentType::Calculator` (`GATE_F6 calculator userspace`).
32. **Gate I4 SMP balance** — unpinned Ring 3 `getcpu` runs on idle CPU 1 without claiming BSP CFS current (`GATE_I4 smp balance`).
33. **Gate G4 audit** — `set_robust_list`/`get_robust_list` round-trip; `madvise` EINVAL; `mlock` records; `readahead`/`fadvise64` return EBADF (`GATE_G4 audit live`).
34. **Gate F7 Files userspace** — Ring 3 SHM client, not `WindowContentType::FileExplorer` (`GATE_F7 files userspace`). Settings/Task Manager/Editor/Browser/AI spawn the same SHM class.
35. **Gate F8 VirtIO-GPU** — controlq `RESOURCE_CREATE_2D` + `ATTACH_BACKING` + `SET_SCANOUT` + `TRANSFER_TO_HOST_2D` + `RESOURCE_FLUSH` (`GATE_F8 virtio-gpu scanout`). cursorq `UPDATE_CURSOR` (`GATE_F8 hw cursor`). `-vga virtio` makes that scanout the cocoa/GTK window (`GATE_GPU modern stack`); the painted cursor plane is the pointer (`GATE_GPU pointer plane`).
36. **Gate C8 JBD2** — ramdisk journal: descriptor + data + commit applied; torn descriptor discarded (`GATE_C8 jbd2 replay`). ext4 mount uses the same walker when `fs-ext4` is on.
37. **USB HID URB** — submit + complete + boot-protocol keyboard parse delivers evdev KEY_A (`GATE_USB hid urb`).
38. **XHCI MMIO** — BAR0 mapped uncached via `vmm::map_mmio`; rings are buddy DMA frames, not heap pointers (`GATE_USB xhci mmio`). Transfer Events drain into the HID URB handler and the GUI mouse/key path.
39. **Gate G5** — every `unsafe {` has a `// SAFETY:` comment; `undocumented_unsafe_blocks = deny` (`GATE_G5 safety comments`).
40. **Gate C9 ext4 root** — second virtio-blk is `mkfs.ext4` and mounted at `/`; generated files under `/dev` `/proc` `/sys` `/run` `/tmp` stay virtual (`GATE_C9 ext4 root`). Mount-point directories persist (C10).
41. **USB HID cursor without PS/2** — boot-protocol mouse URB moves `MOUSE.x/y`; i8042 aux is not enabled (`GATE_USB no ps2 cursor`).
42. **Secure Boot QEMU** — host `scripts/qemu-secure-boot.sh` enrolls PK/KEK/db and boots a signed EFI (`GATE_SB ovmf boot`).
43. **Cross-arch QEMU boot** — aarch64/riscv64 `_start` prints `GATE_ARCH aarch64 boot` / `GATE_ARCH riscv64 boot` and idles.

Syscall gates **J–BL** (clone, namespaces, VFS, sockets, ENOSYS honesty) are proven on the same boot path. They are archived under [gates/syscalls.md](gates/syscalls.md), not repeated here.

44. **Gate D6 IPv6 echo** — loopback `::1` Echo Request; reply is TXed (`GATE_D6 ipv6 echo`). RX `0x86DD` feeds the IPv6 stack.
45. **Service restart** — a Failed unit with `OnFailure` is set Running (`GATE_SVC restart`).
46. **udev hotplug** — PCI devices get `/dev` nodes and kobject ADD uevents (`GATE_UDEV hotplug`).
47. **Quota enforce** — hard block limit returns `EDQUOT` (`GATE_QUOTA enforce`).
48. **Cron reboot** — `@reboot` `/usr/sbin/fsck` runs at init (`GATE_CRON reboot`); timer IRQ sets a due flag.
49. **VSync paced** — software VBlank page flip (`GATE_VSYNC paced`).
50. **Gate TERM** — Ring 3 `openpty` + SHM present as Empty (`GATE_TERM pty userspace`). Start-menu Terminal is that client; master fd + slave I/O (`GATE_TERM pty interactive`).
51. **Gate WL** — `wl_display` object protocol on `/run/wayland-0` (`GATE_WL display protocol`).
52. **Gate GPU** — desktop `present_rect` flushes through VirtIO-GPU (`GATE_GPU desktop scanout`).
53. **Memory cgroups** — mmap/brk charge; 8 KiB over a 4 KiB hard limit is ENOMEM (`GATE_CGROUP memory`, `GATE_CGROUP2 memory`).
54. **Job control** — SIGTSTP stops a child; `waitpid(WUNTRACED)` reports stopped; SIGCONT resumes (`GATE_JOB stop`).
55. **IMA** — exec/spawn SHA-256 measurement list (`GATE_IMA exec`).
56. **Lockdown** — Integrity denies unsigned `insert_module` (`GATE_LOCKDOWN module`).
57. **UEFI runtime** — `GetTime` from CMOS RTC; variable round-trip (`GATE_UEFI time`).
58. **Secure wipe** — buddy `free_physical_frame` zeros the page (`GATE_WIPE free`).
59. **NetworkManager** — `activate` copies live `eth0` DHCP into the profile (`GATE_NM dhcp`).
60. **NTFS** — ramdisk boot + MFT `$DATA` (`GATE_NTFS block`).
61. **Here-docs / profile** — `cat <<EOF` stdin + `~/.profile` (`GATE_HEREDOC expand`, `GATE_PROFILE sourced`).
62. **VT100** — CUP + xterm-256 SGR + ED (`GATE_VT100 xterm`).
63. **HDA + PulseAudio** — buddy DMA + stream mix (`GATE_HDA dma`, `GATE_PA mix`).
64. **IOMMU** — `enable_translation` + IOVA map (`GATE_IOMMU translation`).
65. **THP / KASLR** — 2 MiB hugepage + module slide (`GATE_THP hugepage`, `GATE_KASLR slide`).
66. **ACPI AML** — DSDT `_S5` (`GATE_AML dsdt`).
67. **`.ko` loader** — ET_REL `init_module` (`GATE_KO load`).
68. **NUMA** — `allocate_node` returns a buddy frame charged to the node (`GATE_NUMA frame`).
69. **KSM** — identical pages hashed via `phys_to_virt` collapse; victim is freed (`GATE_KSM merge`).
70. **Nested IRQ / TPR** — ISRs raise LAPIC TPR; nested enter writes TPR (`GATE_NIRQ tpr`).
71. **Live patch** — dedicated 4K trampoline, never W+X, activeness wait (`GATE_LPATCH wx`).
72. **exFAT** — VBR + FAT + directory `$DATA` through the ramdisk (`GATE_EXFAT block`).
73. **cgroup CPU/IO** — `cpu.max` quota and `io.max` byte cap deny (`GATE_CGROUP cpu`, `GATE_CGROUP io`).
74. **Suspend / logind** — S3 save/restore hooks (`GATE_SUSPEND resume`); session create/list/terminate (`GATE_LOGIND session`).
75. **AES-GCM / Poly1305** — NIST AES-128-GCM and RFC 8439 Poly1305 (`GATE_AESGCM kat`, `GATE_POLY1305 kat`).
76. **Font fallback** — compositor chain used for CJK/emoji (`GATE_FONT fallback`).
77. **XFS** — superblock + inode + extent `$DATA` through the ramdisk (`GATE_XFS block`).
78. **X25519 / Ed25519** — RFC 7748 Alice public + RFC 8032 TEST 1 (`GATE_X25519 kat`, `GATE_ED25519 kat`).
79. **Locale** — GNU `.mo` load + `set_locale` + gettext (`GATE_LOCALE switch`).
80. **IME** — pinyin `"ni"` → `你` (`GATE_IME compose`).
81. **USB MSC** — SCSI READ(10) vs ramdisk LUN (`GATE_USB msc`).
82. **NAT** — IPv4 SNAT/DNAT with header checksum (`GATE_NAT masquerade`).
83. **DRM** — GEM from buddy frames + CRTC set (`GATE_DRM gem`).
84. **GSUB** — Latin `fi` ligature (`GATE_GSUB liga`).
85. **RSA / ECDSA** — PKCS#1 v1.5 SHA-256 + RFC 6979 P-256 (`GATE_RSA kat`, `GATE_ECDSA kat`).
86. **Color emoji** — U+1F600 multi-color bitmap (`GATE_EMOJI color`).
87. **USB hub / gamepad** — hub descriptor + port status (`GATE_USB hub`); HID report (`GATE_USB gamepad`).
88. **TPM TIS** — locality 0 handshake (`GATE_TPM tis`).
89. **TrueType hinting** — MDAP round-to-grid (`GATE_TTF hint`).
90. **I2C** — SMBus EEPROM round-trip (`GATE_I2C xfer`).
91. **TLS 1.3 HKDF** — RFC 5869 SHA-256 + Expand-Label (`GATE_TLS13 hkdf`).
92. **HDR / VRR** — PQ EOTF 1.0 → 10_000 nits; Adaptive-Sync 48–144 Hz (`GATE_HDR pq`, `GATE_VRR range`).
93. **e1000** — `Socket::send` TX ring + loopback RX (`GATE_E1000 send`).
94. **eBPF / sched_ext** — `r0=42` plus pick hook (`GATE_EBPF exec`).
95. **Wi-Fi** — Beacon SSID + WPA2 PMK (`GATE_WIFI pmk`).
96. **i915** — pipe A + GTT modeset (`GATE_I915 modeset`).
97. **Btrfs** — superblock + inode + extent `$DATA` (`GATE_BTRFS block`).
98. **VLAN** — 802.1Q insert/strip (`GATE_VLAN tag`).
99. **USB Audio** — UAC SET_CUR 48 kHz + isochronous PCM loopback (`GATE_USB audio`).
100. **Trackpad / Wacom / touch** — HID two-finger scroll (`GATE_USB trackpad`); Intuos pen (`GATE_USB wacom`); digitizer absolute (`GATE_USB touch`).
101. **UVC / USB Ethernet / GPIO** — YUY2 EOF payload (`GATE_USB uvc`); CDC ECM loopback (`GATE_USB ethernet`); pin loopback (`GATE_GPIO xfer`).
102. **rtl8139** — TX copies into the RX ring with a Realtek ROK header (`GATE_RTL8139 send`).
103. **CIFS** — SMB2 Negotiate parse + TREE WRITE/READ (`GATE_CIFS smb2`).
104. **GPOS** — A+V pair-adjustment shrinks advance (`GATE_GPOS kern`).
105. **AMDGPU** — PSP+SMU+SDMA+GFX firmware + CRTC0 enable (`GATE_AMDGPU fw`).
106. **AX211** — iwlwifi TLV firmware + Beacon GATE (`GATE_AX211 fw`).
107. **FUSE** — LOOKUP+READ through the in-kernel loopback daemon (`GATE_FUSE lookup`).
108. **KVM** — software 4-level EPT + HLT `0xF4` VM-exit + virtio-mmio magic (`GATE_KVM ept`, `GATE_KVM vmlaunch`).
109. **OCI** — `runc start` unshare + `pivot_root` (`GATE_OCI start`).
110. **ZFS** — uberblock `0x00BAB10C` + Fletcher-4 ramdisk (`GATE_ZFS block`).
111. **WPA3 / bridge** — SAE matching PMK (`GATE_WPA3 sae`); FDB learn/forward (`GATE_BRIDGE fdb`).
112. **TLS 1.3 handshake** — ClientHello+ServerHello matching `c hs traffic` (`GATE_TLS13 hs`).
113. **GPU matmul** — CPU KAT `[[19,22],[43,50]]` (`GATE_GPU matmul`).
114. **AI** — scaled-dot-product attention + KV append (`GATE_AI attn`); assistant summarize (`GATE_AI chat`).
115. **NFS** — COMPOUND PUTROOTFH+LOOKUP+READ (`GATE_NFS compound`).
116. **Broadcom / Thunderbolt** — `BRCMFW` TLV + scan (`GATE_BRCM fw`); TB route hop (`GATE_TB hop`).
117. **X.509** — DER parse + RSA PKCS#1 v1.5 SHA-256 over TBS (`GATE_TLS13 x509`).
118. **dpkg** — ar + tar install of a fixture `.deb` (`GATE_DPKG deb`).
119. **TPM PCR** — software SHA-256 extend (`GATE_TPM pcr`); IMA extends PCR 10.
120. **BPE vocab** — encode/decode `"the"` (`GATE_AI vocab`).
121. **CPIO newc** — initramfs payload round-trip (`GATE_CPIO newc`).
122. **Title `'i'`** — UiBold falls back so `"Terminal"` keeps the stem (`GATE_FONT title`).
123. **Terminal SHM paint** — first frame glyphs, scaled into the client area (`GATE_TERM shm paint`).
124. **SELinux** — Enforcing TE on VFS: `container_t` writes a `container_t` path and is denied `/etc` (`GATE_SELINUX deny`). Default remains Permissive.
125. **XHCI Enable Slot** — scratchpad + Command Completion cc=1 (`GATE_USB xhci slot`).
126. **NTP** — Server-mode fixture applies a 1000 ms offset (`GATE_NTP apply`).
127. **Bluetooth HCI** — virtual adapter HCI Reset Command Complete (`GATE_BT hci`).
128. **dm-verity** — SHA-256 Merkle leaf equals root; tamper is rejected (`GATE_VERITY root`).
129. **AppArmor** — `network-daemon` writes `/run` and is denied `/etc` (`GATE_APPARMOR deny`). Default processes stay unconfined.
130. **XHCI GET_DESCRIPTOR** — chained Setup+Data+Status TD; Transfer Event required (`GATE_USB xhci desc`).
131. **Service cgroup** — unit `MemoryMax=4KiB` denies 8KiB (`GATE_SVC cgroup`).
132. **GPOS mark** — `e` + combining acute attaches with zero mark advance (`GATE_GPOS mark`).
133. **io_uring** — FIFO SQE; NOP + WRITE CQEs (`GATE_IOURING sqe`). Ring 3 stays ENOSYS (M4).
134. **fanotify** — VFS write queues `FAN_CREATE`/`FAN_MODIFY` (`GATE_FANOTIFY modify`). Ring 3 stays ENOSYS (P4).
135. **D-Bus Hello** — `org.freedesktop.DBus.Hello` returns a `:1.` unique name (`GATE_DBUS method`).
136. **XHCI SET_CONFIGURATION** — Setup+Status Transfer Event (`GATE_USB xhci config`).
137. **XHCI Address Device** — Command Completion cc=1 (`GATE_USB xhci addr`).
138. **D-Bus RequestName** — PRIMARY_OWNER plus second claimant EXISTS (`GATE_DBUS name`).
139. **FUSE WRITE** — WRITE then READ round-trip (`GATE_FUSE write`).
140. **PAM unix** — matching SHA-512 token succeeds; wrong token is AuthError (`GATE_PAM unix`).
141. **cgroup freeze** — frozen cgroup denies CPU charge (`GATE_CGROUP freeze`).
142. **XHCI enum** — Enable Slot, Address Device, GET_DESCRIPTOR, GET_DESCRIPTOR(Configuration), and SET_CONFIGURATION each require a live completion; timeout fallbacks are gone (`GATE_USB xhci enum`).
143. **Virtual FS persist** — `/dev` `/proc` `/sys` `/run` `/tmp` directories round-trip through VirtIO-blk; generated files under them stay virtual (`GATE_C10 virtfs persist`).
144. **ACPI S5** — `acpi::shutdown` writes PM1a_CNT with DSDT `_S5` SLP_TYP|SLP_EN (`GATE_ACPI s5`).
145. **FAT32** — BPB + directory `$DATA` through the ramdisk (`GATE_FAT32 block`).
146. **ext2** — superblock + inode `$DATA` through the ramdisk (`GATE_EXT2 block`).
147. **ISO9660** — PVD + directory `$DATA` through the ramdisk (`GATE_ISO9660 block`).
148. **XHCI Configure Endpoint** — Command Completion cc=1 (`GATE_USB xhci ep`).
149. **D-Bus ReleaseName** — ownership drop then PRIMARY_OWNER (`GATE_DBUS release`).
150. **FUSE UNLINK** — LOOKUP fails after UNLINK (`GATE_FUSE unlink`).
151. **FUSE MKDIR** — LOOKUP returns `S_IFDIR` (`GATE_FUSE mkdir`).
152. **D-Bus ListNames** — well-known name is in the array after `RequestName` (`GATE_DBUS list`).
153. **XHCI GET_STATUS** — Setup+Data+Status Transfer Event (`GATE_USB xhci status`).
154. **FAT32 mount** — `mount -t vfat` parses the BPB and reads `$DATA` (`GATE_FAT32 mount`).
155. **NFS WRITE** — LOOKUP+WRITE then READ round-trip (`GATE_NFS write`).
156. **FUSE RMDIR** — LOOKUP fails after RMDIR (`GATE_FUSE rmdir`).
157. **FUSE RENAME** — LOOKUP finds the new name (`GATE_FUSE rename`).
158. **D-Bus GetConnectionUnixProcessID** — unique and well-known names return the pid (`GATE_DBUS pid`).
159. **XHCI SET_INTERFACE** — Setup+Status Transfer Event (`GATE_USB xhci iface`).
160. **ISO9660 mount** — `mount -t iso9660` parses the PVD and reads `$DATA` (`GATE_ISO9660 mount`).
161. **NTFS mount** — `mount -t ntfs` parses the BPB and reads `$DATA` (`GATE_NTFS mount`).
162. **NFS REMOVE** — REMOVE then LOOKUP is `NFS4ERR_NOENT` (`GATE_NFS remove`).
163. **FUSE SYMLINK** — LOOKUP returns `S_IFLNK`; READLINK returns the target (`GATE_FUSE symlink`).
164. **FUSE LINK** — LOOKUP finds the new name with the same inode (`GATE_FUSE link`).
165. **D-Bus GetNameOwner** — well-known name returns the owner's unique name (`GATE_DBUS owner`).
166. **XHCI GET_CONFIGURATION** — Setup+Data+Status Transfer Event (`GATE_USB xhci getcfg`).
167. **exFAT mount** — `mount -t exfat` parses the VBR and reads `$DATA` (`GATE_EXFAT mount`).
168. **Btrfs mount** — `mount -t btrfs` parses the superblock and reads `$DATA` (`GATE_BTRFS mount`).
169. **NFS RENAME** — RENAME then LOOKUP finds the new name (`GATE_NFS rename`).
170. **FUSE CREATE** — LOOKUP returns `S_IFREG` (`GATE_FUSE create`).
171. **FUSE GETATTR** — size and `S_IFREG` for a created file (`GATE_FUSE getattr`).
172. **D-Bus NameHasOwner** — false before `RequestName`, true after (`GATE_DBUS hasowner`).
173. **XHCI GET_INTERFACE** — Setup+Data+Status Transfer Event (`GATE_USB xhci getiface`).
174. **XFS mount** — `mount -t xfs` parses the superblock and reads `$DATA` (`GATE_XFS mount`).
175. **ZFS mount** — `mount -t zfs` parses the uberblock and reads `$DATA` (`GATE_ZFS mount`).
176. **NFS CREATE** — CREATE then LOOKUP finds the new name (`GATE_NFS create`).
177. **FUSE SETATTR** — size change then GETATTR/READ (`GATE_FUSE setattr`).
178. **FUSE READDIR** — root lists created names (`GATE_FUSE readdir`).
179. **D-Bus GetConnectionUnixUser** — unique and well-known names return the uid (`GATE_DBUS uid`).
180. **D-Bus AddMatch** — matching signals are delivered (`GATE_DBUS match`).
181. **XHCI CLEAR_FEATURE** — Setup+Status Transfer Event (`GATE_USB xhci clear`).
182. **XHCI bulk** — bulk OUT Normal TRB Transfer Event (`GATE_USB xhci bulk`).
183. **NFS LINK** — SAVEFH+LINK then LOOKUP finds both names (`GATE_NFS link`).
184. **NFS READLINK** — CREATE NF4LNK then READLINK returns the target (`GATE_NFS readlink`).
185. **FUSE MKNOD** — LOOKUP returns `S_IFCHR` (`GATE_FUSE mknod`).
186. **FUSE STATFS** — `bsize` 4096 and created-name count (`GATE_FUSE statfs`).
187. **D-Bus RemoveMatch** — matching signals stop after the inverse of AddMatch (`GATE_DBUS remove`).
188. **D-Bus GetId** — 32-hex machine UUID (`GATE_DBUS id`).
189. **XHCI SET_FEATURE** — Setup+Status Transfer Event (`GATE_USB xhci set`).
190. **XHCI bulk IN** — bulk IN Normal TRB Transfer Event (`GATE_USB xhci bulkin`).
191. **NFS SETATTR** — size change then GETATTR/READ (`GATE_NFS setattr`).
192. **NFS READDIR** — export root lists created names (`GATE_NFS readdir`).
193. **FUSE SETXATTR** — SETXATTR then GETXATTR returns the value (`GATE_FUSE setxattr`).
194. **FUSE GETXATTR** — stored value plus missing-name failure (`GATE_FUSE getxattr`).
195. **D-Bus StartServiceByName** — SUCCESS then ALREADY_RUNNING (`GATE_DBUS start`).
196. **D-Bus GetConnectionCredentials** — unique and well-known names return pid+uid (`GATE_DBUS creds`).
197. **XHCI SET_DESCRIPTOR** — Setup+Data+Status Transfer Event (`GATE_USB xhci stdesc`).
198. **XHCI isochronous IN** — Isoch TRB Transfer Event (`GATE_USB xhci isoch`).
199. **NFS ACCESS** — file bits from mode, not an echo of the request (`GATE_NFS access`).
200. **NFS COMMIT** — unstable WRITE then COMMIT then READ (`GATE_NFS commit`).
201. **FUSE LISTXATTR** — LISTXATTR includes a SETXATTR name (`GATE_FUSE listxattr`).
202. **FUSE REMOVEXATTR** — drops a stored name so GETXATTR fails (`GATE_FUSE removexattr`).
203. **D-Bus ListActivatableNames** — seeded well-known name is listed (`GATE_DBUS activate`).
204. **D-Bus ListQueuedOwners** — queued RequestName unique name (`GATE_DBUS queue`).
205. **XHCI GET_DESCRIPTOR(String)** — Setup+Data+Status Transfer Event (`GATE_USB xhci string`).
206. **XHCI interrupt IN** — Interrupt IN Normal TRB Transfer Event (`GATE_USB xhci intr`).
207. **NFS GETFH** — LOOKUP then GETFH is a child handle (`GATE_NFS getfh`).
208. **NFS PUTFH** — restore GETFH handle; unknown is STALE (`GATE_NFS putfh`).
209. **Ring 3 calc key** — attached SHM calculator re-rasters on `'7'` (`GATE_GPUI ring3 calc key`).
210. **musl raster** — musl CRT `main` calls `KnoxUiRaster` and presents GPUM (`GATE_GPUI musl raster`).
211. **`/bin/sh` parser** — Ring 3 `/bin/sh` calls `ShellExecute`; kernel `compat/shell` implements it (`GATE_SH ring3 parser`).
212. **D-Bus daemon** — Ring 3 process bind+listen `/run/dbus/system_bus_socket` (`GATE_DBUS daemon userspace`).
213. **Wayland server** — Ring 3 process owns `/run/wayland-0`; compositor stays in-kernel (`GATE_WL server userspace`).
214. **FUSE ACCESS** — granted bits from mode, not an echo of the request (`GATE_FUSE access`).
215. **FUSE FSYNC** — WRITE then FSYNC; unknown nodeid fails (`GATE_FUSE fsync`).
216. **D-Bus SELinux context** — unique and well-known names return the context (`GATE_DBUS selinux`).
217. **D-Bus AppArmor profile** — unique and well-known names return `unconfined` (`GATE_DBUS apparmor`).
218. **XHCI No-Op Command** — Command Completion cc=1 (`GATE_USB xhci noop`).
219. **XHCI Disable Slot** — Command Completion cc=1 (`GATE_USB xhci disable`).
220. **NFS LOOKUPP** — child LOOKUPP is the export root; root LOOKUPP is NOENT (`GATE_NFS lookupp`).
221. **NFS RESTOREFH** — SAVEFH then RESTOREFH; missing is NOFILEHANDLE (`GATE_NFS restorefh`).
222. **FUSE OPEN** — known inode returns fh=nodeid; unknown fails (`GATE_FUSE open`).
223. **FUSE RELEASE** — OPEN then RELEASE; missing nodeid or mismatched fh fails (`GATE_FUSE release`).
224. **D-Bus ADT session** — unique and well-known names return the session bytes (`GATE_DBUS adt`).
225. **D-Bus activation environment** — `UpdateActivationEnvironment` method return (`GATE_DBUS env`).
226. **XHCI Evaluate Context** — Command Completion cc=1 (`GATE_USB xhci eval`).
227. **XHCI Stop Endpoint** — bulk OUT (DCI 2) Command Completion cc=1 (`GATE_USB xhci stop`).
228. **NFS PUTPUBFH** — after LOOKUP, PUTPUBFH is the export root (`GATE_NFS putpubfh`).
229. **NFS VERIFY** — matching size OK; mismatch NOT_SAME; missing FH NOFILEHANDLE (`GATE_NFS verify`).
230. **FUSE FLUSH** — WRITE then FLUSH; unknown nodeid fails (`GATE_FUSE flush`).
231. **FUSE FSYNCDIR** — directory succeeds; file or unknown nodeid fails (`GATE_FUSE fsyncdir`).
232. **D-Bus ReloadConfig** — empty method return (`GATE_DBUS reload`).
233. **D-Bus BecomeMonitor** — method return, then signals without AddMatch (`GATE_DBUS monitor`).
234. **XHCI Reset Endpoint** — Halted bulk IN (DCI 5) Command Completion cc=1 (`GATE_USB xhci resetep`).
235. **XHCI Set TR Dequeue Pointer** — Stopped extra pipe Command Completion cc=1 (`GATE_USB xhci settr`).
236. **NFS NVERIFY** — mismatch OK; match SAME; missing FH NOFILEHANDLE (`GATE_NFS nverify`).
237. **NFS SETCLIENTID_CONFIRM** — issued client_id OK; wrong id STALE_CLIENTID (`GATE_NFS confirm`).
238. **FUSE OPENDIR** — directory returns fh=nodeid; file or unknown fails (`GATE_FUSE opendir`).
239. **FUSE RELEASEDIR** — OPENDIR then RELEASEDIR; file, unknown, or mismatched fh fails (`GATE_FUSE releasedir`).
240. **D-Bus Peer Ping** — empty method return (`GATE_DBUS ping`).
241. **D-Bus Peer GetMachineId** — 32-hex machine UUID (`GATE_DBUS machineid`).
242. **XHCI Reset Device** — already-addressed slot Command Completion cc=1 (`GATE_USB xhci resetdev`).
243. **XHCI Transfer No-Op** — bulk OUT (DCI 2) No-Op TRB Transfer Event (`GATE_USB xhci trnoop`).
244. **NFS CLOSE** — issued OPEN state_id OK; wrong id BAD_STATEID; missing FH NOFILEHANDLE (`GATE_NFS close`).
245. **NFS RENEW** — confirmed client_id OK; wrong id STALE_CLIENTID (`GATE_NFS renew`).
246. **FUSE GETLK** — known file with no lock is F_UNLCK; unknown nodeid fails (`GATE_FUSE getlk`).
247. **FUSE SETLK** — F_WRLCK is visible to GETLK from another pid; F_UNLCK drops it (`GATE_FUSE setlk`).
248. **D-Bus Introspect** — XML names Introspectable and Peer (`GATE_DBUS introspect`).
249. **D-Bus Properties.Get** — Features then UnknownProperty (`GATE_DBUS props`).
250. **XHCI Device Qualifier** — GET_DESCRIPTOR(Device Qualifier) Transfer Event (`GATE_USB xhci qual`).
251. **XHCI BOS** — GET_DESCRIPTOR(BOS) Transfer Event (`GATE_USB xhci bos`).
252. **NFS LOCK** — free range OK; overlap DENIED; missing FH NOFILEHANDLE (`GATE_NFS lock`).
253. **NFS LOCKU** — held range OK; missing range DENIED; missing FH NOFILEHANDLE (`GATE_NFS unlock`).
254. **FUSE SETLKW** — F_WRLCK is visible to GETLK from another pid; F_UNLCK drops it (`GATE_FUSE setlkw`).
255. **FUSE BMAP** — logical block 0 maps to nodeid; unknown nodeid fails (`GATE_FUSE bmap`).
256. **D-Bus Properties.GetAll** — Features then UnknownInterface (`GATE_DBUS getall`).
257. **D-Bus Properties.Set** — Features PropertyReadOnly then UnknownProperty (`GATE_DBUS setprop`).
258. **XHCI Other Speed Configuration** — GET_DESCRIPTOR(Other Speed Configuration) Transfer Event (`GATE_USB xhci oscfg`).
259. **XHCI SYNCH_FRAME** — SYNCH_FRAME Transfer Event (`GATE_USB xhci synch`).
260. **NFS LOCKT** — free range OK; overlap DENIED; missing FH NOFILEHANDLE (`GATE_NFS lockt`).
261. **NFS RELEASE_LOCKOWNER** — drops that owner's locks; missing FH NOFILEHANDLE (`GATE_NFS lockowner`).
262. **FUSE DESTROY** — known mount succeeds; unknown mount fails (`GATE_FUSE destroy`).
263. **FUSE IOCTL** — known file echoes cmd; unknown nodeid fails (`GATE_FUSE ioctl`).
264. **D-Bus Debug.Stats.GetStats** — Connections count (`GATE_DBUS stats`).
265. **D-Bus GetAllMatchRules** — unique name after AddMatch (`GATE_DBUS matchrules`).
266. **XHCI SET_SEL** — SET_SEL Transfer Event (`GATE_USB xhci setsel`).
267. **XHCI SET_ISOCH_DELAY** — SET_ISOCH_DELAY Transfer Event (`GATE_USB xhci isochdelay`).
268. **NFS OPEN_CONFIRM** — issued OPEN OK; wrong id BAD_STATEID; missing FH NOFILEHANDLE (`GATE_NFS openconfirm`).
269. **NFS OPEN_DOWNGRADE** — subset OK; superset INVAL; missing FH NOFILEHANDLE (`GATE_NFS downgrade`).
270. **FUSE POLL** — known file with data is POLLIN; unknown nodeid fails (`GATE_FUSE poll`).
271. **FUSE FALLOCATE** — mode 0 extends size; unknown nodeid fails (`GATE_FUSE fallocate`).
272. **D-Bus GetConnectionStats** — UnixProcessID; unknown is NameHasNoOwner (`GATE_DBUS connstats`).
273. **D-Bus GetManagedObjects** — `/org/freedesktop/DBus` (`GATE_DBUS objects`).
274. **XHCI HID** — GET_DESCRIPTOR(HID) Transfer Event (`GATE_USB xhci hiddesc`).
275. **XHCI HID Report** — GET_DESCRIPTOR(HID Report) Transfer Event (`GATE_USB xhci hidreport`).
276. **NFS SECINFO** — known name AUTH_SYS; missing NOENT; missing FH NOFILEHANDLE (`GATE_NFS secinfo`).
277. **NFS OPENATTR** — createdir OK; without is NOENT; missing FH NOFILEHANDLE (`GATE_NFS openattr`).
278. **FUSE READDIRPLUS** — root lists names with size; unknown nodeid fails (`GATE_FUSE readdirplus`).
279. **FUSE RENAME2** — flags=0 moves; NOREPLACE dest fails; unknown old fails (`GATE_FUSE rename2`).
280. **D-Bus NameOwnerChanged** — after RequestName (`GATE_DBUS ownerchanged`).
281. **D-Bus NameLost** — after ReleaseName (`GATE_DBUS namelost`).
282. **XHCI HID GET_REPORT** — class IN Transfer Event (`GATE_USB xhci getreport`).
283. **XHCI HID SET_REPORT** — class OUT Transfer Event (`GATE_USB xhci setreport`).
284. **NFS DELEGPURGE** — confirmed client_id OK; wrong id STALE_CLIENTID (`GATE_NFS delegpurge`).
285. **NFS DELEGRETURN** — OPEN state_id OK; second BAD_STATEID; missing FH NOFILEHANDLE (`GATE_NFS delegreturn`).
286. **FUSE LSEEK** — SEEK_SET 0 is 0; SEEK_END is size; unknown nodeid fails (`GATE_FUSE lseek`).
287. **FUSE COPY_FILE_RANGE** — copies bytes between known files; unknown nodeid fails (`GATE_FUSE copy`).
288. **D-Bus NameAcquired** — after RequestName (`GATE_DBUS nameacquired`).
289. **D-Bus GetConnectionLinuxSecurityLabel** — LSM label unique+well-known (`GATE_DBUS label`).
290. **XHCI HID GET_IDLE** — class IN Transfer Event (`GATE_USB xhci getidle`).
291. **XHCI HID SET_IDLE** — class OUT Transfer Event (`GATE_USB xhci setidle`).
292. **NFS EXCHANGE_ID** — client_name returns client_id; empty is INVAL (`GATE_NFS exchangeid`).
293. **NFS CREATE_SESSION** — EXCHANGE_ID client_id OK; wrong id STALE_CLIENTID (`GATE_NFS session`).
294. **FUSE SYNCFS** — known mount succeeds; unknown mount fails (`GATE_FUSE syncfs`).
295. **FUSE STATX** — size and `S_IFREG` for a known file; unknown nodeid fails (`GATE_FUSE statx`).
296. **D-Bus GetConfigFile** — `/etc/dbus-1/system.conf` (`GATE_DBUS config`).
297. **D-Bus ActivatableServicesChanged** — after ReloadConfig (`GATE_DBUS svcchanged`).
298. **XHCI HID GET_PROTOCOL** — class IN Transfer Event (`GATE_USB xhci getprotocol`).
299. **XHCI HID SET_PROTOCOL** — class OUT Transfer Event (`GATE_USB xhci setprotocol`).
300. **NFS DESTROY_SESSION** — CREATE_SESSION id OK; second/unknown BADSESSION (`GATE_NFS destroysession`).
301. **NFS DESTROY_CLIENTID** — live session CLIENTID_BUSY; after DESTROY_SESSION OK; wrong id STALE_CLIENTID (`GATE_NFS destroyclientid`).

Integration: `./tests/run_integration.sh` **587/587** on QEMU (2026-10-24).
