← [Status index](../README.md) · [Do this first](../next.md)

# 10. System Services

**Grade: Wired (85%)** · `init.rs`, `service_manager.rs`, `dbus.rs`, `cron.rs`, `pam.rs`, `ntp.rs`

### Wired
- [x] Init searches standard paths; can map a built-in ELF blob
- [x] **Ring 3 `/sbin/init`** — boot path `iretq`s to a static init ELF that writes and `wait4`s (`GATE_K3 init userspace`)
- [x] Service manager `boot()` from `main.rs`; dependency-ish unit states
- [x] In-memory D-Bus routing (`send_message`)
- [x] **AF_UNIX system bus** — `/run/dbus/system_bus_socket` bind/listen/connect/send/recv (`GATE_L3 dbus unix`)
- [x] **D-Bus daemon as a user process** on `/run/dbus/system_bus_socket` (`GATE_DBUS daemon userspace`); kernel `uds` remains for L3/host tests
- [x] Cron field parser + `tick` → execute builtin/ELF
- [x] dmesg ring, syslog structures
- [x] PAM salted SHA-512 verify; session/seat structs
- [x] ACPI S5 through PM1a_CNT (`GATE_ACPI s5`)
- [x] NTP fixture Server-mode packet applies a 1000 ms offset (`GATE_NTP apply`)
- [x] Bluetooth HCI virtual adapter Reset Command Complete (`GATE_BT hci`)

### Stub
- [x] Init `iretq`s to `/sbin/init` (K3); Failed units restart (`GATE_SVC restart`)
- [x] Crash restart of Failed units (`GATE_SVC restart`). Unit `MemoryMax=` cgroup (`GATE_SVC cgroup`).
- [x] D-Bus listens on `AF_UNIX` `/run/dbus/system_bus_socket` (L3); `Hello` returns a `:1.` unique name (`GATE_DBUS method`). `RequestName` returns PRIMARY_OWNER (`GATE_DBUS name`). `ReleaseName` drops ownership (`GATE_DBUS release`). `ListNames` includes the well-known name (`GATE_DBUS list`). `GetConnectionUnixProcessID` returns the pid (`GATE_DBUS pid`). `GetNameOwner` returns the unique name (`GATE_DBUS owner`). `NameHasOwner` is false then true (`GATE_DBUS hasowner`). `GetConnectionUnixUser` returns the uid (`GATE_DBUS uid`). `AddMatch` delivers matching signals (`GATE_DBUS match`). `RemoveMatch` stops delivery (`GATE_DBUS remove`). `GetId` returns the machine UUID (`GATE_DBUS id`). `StartServiceByName` is SUCCESS then ALREADY_RUNNING (`GATE_DBUS start`). `GetConnectionCredentials` returns pid and uid (`GATE_DBUS creds`). `ListActivatableNames` includes the seeded name (`GATE_DBUS activate`). `ListQueuedOwners` returns a queued unique name (`GATE_DBUS queue`). `GetConnectionSELinuxSecurityContext` returns the context (`GATE_DBUS selinux`). `GetConnectionAppArmorSecurityContext` returns `unconfined` (`GATE_DBUS apparmor`). `GetAdtAuditSessionData` returns the session bytes (`GATE_DBUS adt`). `UpdateActivationEnvironment` returns a method reply (`GATE_DBUS env`). `ReloadConfig` returns a method reply (`GATE_DBUS reload`). `BecomeMonitor` then signals without AddMatch (`GATE_DBUS monitor`). `Peer.Ping` method return (`GATE_DBUS ping`). `Peer.GetMachineId` 32-hex UUID (`GATE_DBUS machineid`). `Introspectable.Introspect` XML (`GATE_DBUS introspect`). `Properties.Get` Features then UnknownProperty (`GATE_DBUS props`). `Properties.GetAll` Features then UnknownInterface (`GATE_DBUS getall`). `Properties.Set` Features PropertyReadOnly then UnknownProperty (`GATE_DBUS setprop`). `Debug.Stats.GetStats` Connections (`GATE_DBUS stats`). `GetAllMatchRules` after AddMatch (`GATE_DBUS matchrules`). `GetConnectionStats` UnixProcessID (`GATE_DBUS connstats`). `GetManagedObjects` `/org/freedesktop/DBus` (`GATE_DBUS objects`). `NameOwnerChanged` after RequestName (`GATE_DBUS ownerchanged`). `NameLost` after ReleaseName (`GATE_DBUS namelost`). `NameAcquired` after RequestName (`GATE_DBUS nameacquired`). `GetConnectionLinuxSecurityLabel` returns the LSM label (`GATE_DBUS label`). `GetConfigFile` returns the system bus path (`GATE_DBUS config`). `ActivatableServicesChanged` after ReloadConfig (`GATE_DBUS svcchanged`). `PropertiesChanged` after ReloadConfig (`GATE_DBUS propschanged`). `InterfacesAdded` after RequestName (`GATE_DBUS ifacesadded`). Not a full bus daemon.
- [x] PAM unix SHA-512: matching token succeeds; wrong token is AuthError (`GATE_PAM unix`)
- [x] Cron `@reboot` + PIT/APIC due-flag (`GATE_CRON reboot`)
- [x] udev-style hotplug → `/dev` from PCI (`GATE_UDEV hotplug`)
- [x] NetworkManager `activate` copies live `eth0` DHCP (`GATE_NM dhcp`)
- [x] Suspend/hibernate, logind — S3 save/restore hooks (`GATE_SUSPEND resume`); session create/list/terminate (`GATE_LOGIND session`)
- [x] PulseAudio mixing to HDA (`GATE_PA mix`)

---
