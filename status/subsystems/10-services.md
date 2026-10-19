← [Status index](../README.md) · [Do this first](../next.md)

# 10. System Services

**Grade: Wired (85%)** · `init.rs`, `service_manager.rs`, `dbus.rs`, `cron.rs`, `pam.rs`, `ntp.rs`

### Wired
- [x] Init searches standard paths; can map a built-in ELF blob
- [x] **Ring 3 `/sbin/init`** — boot path `iretq`s to a static init ELF that writes and `wait4`s (`GATE_K3 init userspace`)
- [x] Service manager `boot()` from `main.rs`; dependency-ish unit states
- [x] In-memory D-Bus routing (`send_message`)
- [x] **AF_UNIX system bus** — `/run/dbus/system_bus_socket` bind/listen/connect/send/recv (`GATE_L3 dbus unix`)
- [x] Cron field parser + `tick` → execute builtin/ELF
- [x] dmesg ring, syslog structures
- [x] PAM salted SHA-512 verify; session/seat structs
- [x] ACPI S5 through PM1a_CNT (`GATE_ACPI s5`)
- [x] NTP fixture Server-mode packet applies a 1000 ms offset (`GATE_NTP apply`)
- [x] Bluetooth HCI virtual adapter Reset Command Complete (`GATE_BT hci`)

### Stub
- [x] Init `iretq`s to `/sbin/init` (K3); Failed units restart (`GATE_SVC restart`)
- [x] Crash restart of Failed units (`GATE_SVC restart`). Unit `MemoryMax=` cgroup (`GATE_SVC cgroup`).
- [x] D-Bus listens on `AF_UNIX` `/run/dbus/system_bus_socket` (L3); `Hello` returns a `:1.` unique name (`GATE_DBUS method`). `RequestName` returns PRIMARY_OWNER (`GATE_DBUS name`). `ReleaseName` drops ownership (`GATE_DBUS release`). `ListNames` includes the well-known name (`GATE_DBUS list`). `GetConnectionUnixProcessID` returns the pid (`GATE_DBUS pid`). `GetNameOwner` returns the unique name (`GATE_DBUS owner`). Not a full bus daemon.
- [x] PAM unix SHA-512: matching token succeeds; wrong token is AuthError (`GATE_PAM unix`)
- [x] Cron `@reboot` + PIT/APIC due-flag (`GATE_CRON reboot`)
- [x] udev-style hotplug → `/dev` from PCI (`GATE_UDEV hotplug`)
- [x] NetworkManager `activate` copies live `eth0` DHCP (`GATE_NM dhcp`)
- [x] Suspend/hibernate, logind — S3 save/restore hooks (`GATE_SUSPEND resume`); session create/list/terminate (`GATE_LOGIND session`)
- [x] PulseAudio mixing to HDA (`GATE_PA mix`)

---
