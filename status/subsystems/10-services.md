← [Status index](../README.md) · [Do this first](../next.md)

# 10. System Services

**Grade: Wired (60%)** · `init.rs`, `service_manager.rs`, `dbus.rs`, `cron.rs`, `pam.rs`

### Wired
- [x] Init searches standard paths; can map a built-in ELF blob
- [x] **Ring 3 `/sbin/init`** — boot path `iretq`s to a static init ELF that writes and `wait4`s (`GATE_K3 init userspace`)
- [x] Service manager `boot()` from `main.rs`; dependency-ish unit states
- [x] In-memory D-Bus routing (`send_message`)
- [x] **AF_UNIX system bus** — `/run/dbus/system_bus_socket` bind/listen/connect/send/recv (`GATE_L3 dbus unix`)
- [x] Cron field parser + `tick` → execute builtin/ELF
- [x] dmesg ring, syslog structures
- [x] PAM salted SHA-512 verify; session/seat structs
- [x] QEMU shutdown ports

### Stub
- [x] Init `iretq`s to `/sbin/init` (K3); Failed units restart (`GATE_SVC restart`)
- [x] Crash restart of Failed units (`GATE_SVC restart`). No cgroup limits on units.
- [x] D-Bus listens on `AF_UNIX` `/run/dbus/system_bus_socket` (L3); not a full bus daemon
- [x] Cron `@reboot` + PIT/APIC due-flag (`GATE_CRON reboot`)
- [x] udev-style hotplug → `/dev` from PCI (`GATE_UDEV hotplug`)
- [x] NetworkManager `activate` copies live `eth0` DHCP (`GATE_NM dhcp`)
- [ ] Suspend/hibernate, logind
- [x] PulseAudio mixing to HDA (`GATE_PA mix`)

---
