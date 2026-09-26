use crate::context::CpuContext;

#[test_case]
fn test_kernel_thread_context_runnable() {
    let ctx = CpuContext::new_kernel_thread(0xFFFF_8000_0010_0000, 0xFFFF_8000_0020_0000);
    assert!(ctx.is_runnable());
    assert_eq!(ctx.cs, 0x08);
    assert_eq!(ctx.ss, 0x10);
    assert_eq!(ctx.rflags & 0x200, 0x200);
    assert_eq!(ctx.rsp & 0xF, 8);
}

#[test_case]
fn test_fxsave_area_alignment() {
    assert_eq!(core::mem::offset_of!(CpuContext, fxsave_area) % 16, 0);
}
