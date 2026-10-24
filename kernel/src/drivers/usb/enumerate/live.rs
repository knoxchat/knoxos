//! Live-event flags set only when a real Command Completion / Transfer Event arrives.
use core::sync::atomic::AtomicBool;

/// True only when Enable Slot returned Command Completion cc=1 (not a timeout fallback).
pub(super) static SLOT_ENABLE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR got a Transfer Event (cc=1/13), not a timeout fallback.
pub(super) static DESC_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_CONFIGURATION got a Transfer Event (cc=1/13), not a timeout fallback.
pub(super) static CONFIG_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Address Device returned Command Completion cc=1 (not a timeout fallback).
pub(super) static ADDRESS_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(Configuration) got a Transfer Event (cc=1/13).
pub(super) static CONFIG_DESC_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Configure Endpoint returned Command Completion cc=1.
pub(super) static CONFIG_EP_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_STATUS got a Transfer Event (cc=1/13).
pub(super) static STATUS_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_INTERFACE got a Transfer Event (cc=1/13).
pub(super) static IFACE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_CONFIGURATION got a Transfer Event (cc=1/13).
pub(super) static GETCFG_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_INTERFACE got a Transfer Event (cc=1/13).
pub(super) static GETIFACE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when CLEAR_FEATURE got a Transfer Event (cc=1/13).
pub(super) static CLEAR_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when a bulk Normal TRB got a Transfer Event (cc=1/13).
pub(super) static BULK_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_FEATURE got a Transfer Event (cc=1/13).
pub(super) static SET_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when a bulk IN Normal TRB got a Transfer Event (cc=1/13).
pub(super) static BULK_IN_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_DESCRIPTOR got a Transfer Event (cc=1/13).
pub(super) static SETDESC_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when an isochronous IN Isoch TRB got a Transfer Event (cc=1/13).
pub(super) static ISOCH_IN_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(String) got a Transfer Event (cc=1/13).
pub(super) static STRING_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when an interrupt IN Normal TRB got a Transfer Event (cc=1/13).
pub(super) static INTR_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when a No-Op Command returned Command Completion cc=1.
pub(super) static NOOP_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Disable Slot returned Command Completion cc=1.
pub(super) static DISABLE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Evaluate Context returned Command Completion cc=1.
pub(super) static EVAL_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Stop Endpoint returned Command Completion cc=1.
pub(super) static STOP_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Reset Endpoint returned Command Completion cc=1.
pub(super) static RESETEP_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Set TR Dequeue Pointer returned Command Completion cc=1.
pub(super) static SETTR_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when Reset Device returned Command Completion cc=1.
pub(super) static RESETDEV_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when a Transfer No-Op TRB got a Transfer Event.
pub(super) static TRNOOP_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(Device Qualifier) got a Transfer Event.
pub(super) static QUAL_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(BOS) got a Transfer Event.
pub(super) static BOS_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(Other Speed Configuration) got a Transfer Event.
pub(super) static OSCFG_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SYNCH_FRAME got a Transfer Event.
pub(super) static SYNCH_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_SEL got a Transfer Event.
pub(super) static SETSEL_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when SET_ISOCH_DELAY got a Transfer Event.
pub(super) static ISOCHDELAY_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(HID) got a Transfer Event.
pub(super) static HID_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when GET_DESCRIPTOR(HID Report) got a Transfer Event.
pub(super) static HIDREPORT_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when HID GET_REPORT got a Transfer Event.
pub(super) static HIDGETREPORT_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when HID SET_REPORT got a Transfer Event.
pub(super) static HIDSETREPORT_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when HID GET_IDLE got a Transfer Event.
pub(super) static HIDGETIDLE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when HID SET_IDLE got a Transfer Event.
pub(super) static HIDSETIDLE_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when HID GET_PROTOCOL got a Transfer Event.
pub(super) static HIDGETPROTOCOL_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when HID SET_PROTOCOL got a Transfer Event.
pub(super) static HIDSETPROTOCOL_LIVE: AtomicBool = AtomicBool::new(false);
/// True only when enumerate_device completed every step from a live event (no fallback).
pub(super) static ENUM_LIVE: AtomicBool = AtomicBool::new(false);
