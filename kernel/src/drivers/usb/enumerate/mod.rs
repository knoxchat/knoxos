//! Device slot enable, addressing, transfer submission, and enumeration.
//!
//! Module layout:
//!   live      — AtomicBool flags set only on a live Command Completion / Transfer Event
//!   command   — command/transfer ring enqueue, completion wait, doorbell
//!   slot      — Enable Slot, Address Device, Configure/Stop/Reset Endpoint, …
//!   events    — interrupt-IN URBs and non-blocking event-ring drain
//!   requests  — standard USB control requests on EP0
//!   pipes     — bulk / isoch / interrupt data pipes and Transfer No-Op
//!   hid       — HID class descriptors and GET/SET_REPORT, IDLE, PROTOCOL
//!   device    — enumerate_device / enumerate_all_devices
//!   gates     — GATE_USB serial markers and self-tests
mod command;
mod device;
mod events;
mod gates;
mod hid;
mod live;
mod pipes;
mod requests;
mod slot;

pub use command::poll_transfer_event;
pub(super) use command::{
    TRB_CH, allocate_transfer_ring, submit_transfer, wait_command_completion,
};
pub use device::{enumerate_all_devices, enumerate_device};
pub use events::{poll_events, submit_interrupt_urb, submit_interrupt_urb_for_hid};
pub use gates::{
    GATE_XHCI_ADDR_MARKER, GATE_XHCI_BOS_MARKER, GATE_XHCI_BULK_MARKER, GATE_XHCI_BULKIN_MARKER,
    GATE_XHCI_CLEAR_MARKER, GATE_XHCI_CONFIG_MARKER, GATE_XHCI_DESC_MARKER,
    GATE_XHCI_DISABLE_MARKER, GATE_XHCI_ENUM_MARKER, GATE_XHCI_EP_MARKER, GATE_XHCI_EVAL_MARKER,
    GATE_XHCI_GETCFG_MARKER, GATE_XHCI_GETIFACE_MARKER, GATE_XHCI_HID_MARKER,
    GATE_XHCI_HIDGETIDLE_MARKER, GATE_XHCI_HIDGETPROTOCOL_MARKER, GATE_XHCI_HIDGETREPORT_MARKER,
    GATE_XHCI_HIDREPORT_MARKER, GATE_XHCI_HIDSETIDLE_MARKER, GATE_XHCI_HIDSETPROTOCOL_MARKER,
    GATE_XHCI_HIDSETREPORT_MARKER, GATE_XHCI_IFACE_MARKER, GATE_XHCI_INTR_MARKER,
    GATE_XHCI_ISOCH_MARKER, GATE_XHCI_ISOCHDELAY_MARKER, GATE_XHCI_NOOP_MARKER,
    GATE_XHCI_OSCFG_MARKER, GATE_XHCI_QUAL_MARKER, GATE_XHCI_RESETDEV_MARKER,
    GATE_XHCI_RESETEP_MARKER, GATE_XHCI_SET_MARKER, GATE_XHCI_SETDESC_MARKER,
    GATE_XHCI_SETSEL_MARKER, GATE_XHCI_SETTR_MARKER, GATE_XHCI_SLOT_MARKER,
    GATE_XHCI_STATUS_MARKER, GATE_XHCI_STATUSEP_MARKER, GATE_XHCI_STATUSIFACE_MARKER,
    GATE_XHCI_STOP_MARKER, GATE_XHCI_STRING_MARKER, GATE_XHCI_SYNCH_MARKER,
    GATE_XHCI_TRNOOP_MARKER, addr_self_test, bos_self_test, bulk_in_self_test, bulk_self_test,
    clear_self_test, config_self_test, descriptor_self_test, disable_self_test, enum_self_test,
    ep_self_test, eval_self_test, getcfg_self_test, getidle_self_test, getiface_self_test,
    getprotocol_self_test, getreport_self_test, hid_self_test, hidreport_self_test,
    iface_self_test, interrupt_in_self_test, isoch_in_self_test, isochdelay_self_test,
    noop_self_test, oscfg_self_test, qual_self_test, resetdev_self_test, resetep_self_test,
    set_descriptor_self_test, set_self_test, setidle_self_test, setprotocol_self_test,
    setreport_self_test, setsel_self_test, settr_self_test, slot_self_test, status_self_test,
    statusep_self_test, statusiface_self_test, stop_self_test, string_self_test, synch_self_test,
    trnoop_self_test,
};
pub use hid::{
    get_hid_descriptor, get_hid_report_descriptor, hid_get_idle, hid_get_protocol, hid_get_report,
    hid_set_idle, hid_set_protocol, hid_set_report,
};
pub use pipes::{bulk_in, bulk_out, interrupt_in, isoch_in, transfer_noop};
pub use requests::{
    clear_feature, get_bos_descriptor, get_config_descriptor, get_configuration,
    get_device_descriptor, get_device_qualifier, get_endpoint_status, get_interface,
    get_interface_status, get_other_speed_config, get_status, get_string_descriptor,
    set_configuration, set_descriptor, set_feature, set_interface, set_isoch_delay, set_sel,
    synch_frame,
};
pub use slot::{
    address_device, configure_endpoint, disable_slot, enable_slot, evaluate_context, no_op_command,
    reset_device, reset_endpoint, set_tr_dequeue, stop_endpoint,
};
