use alloc::string::String;

/// Query the AI assistant with a text prompt
pub fn query(prompt: &str) -> String {
    // Simple pattern-matching response engine
    let prompt_lower = prompt.to_ascii_lowercase();

    if prompt_lower.contains("hello") || prompt_lower.contains("hi") {
        String::from("Hello! I'm KnoxOS AI Assistant. How can I help you today?")
    } else if prompt_lower.contains("time") || prompt_lower.contains("date") {
        let dt = crate::rtc::read_rtc();
        alloc::format!(
            "The current date and time is {}-{:02}-{:02} {:02}:{:02}:{:02} UTC.",
            dt.year,
            dt.month,
            dt.day,
            dt.hour,
            dt.minute,
            dt.second
        )
    } else if prompt_lower.contains("help") {
        String::from(
            "I can help with:\n\
             - System information (ask about 'cpu', 'memory', 'uptime')\n\
             - File operations (ask about 'files', 'directory')\n\
             - Process management (ask about 'processes')\n\
             - Network status (ask about 'network')\n\
             - General questions about KnoxOS",
        )
    } else if prompt_lower.contains("cpu") || prompt_lower.contains("processor") {
        let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
        if let Some(brand) = cpuid.get_processor_brand_string() {
            alloc::format!("CPU: {}", brand.as_str())
        } else {
            String::from("CPU information unavailable")
        }
    } else if prompt_lower.contains("memory") || prompt_lower.contains("ram") {
        alloc::format!(
            "Kernel heap: {} KiB allocated at {:#x}",
            crate::allocator::HEAP_SIZE / 1024,
            crate::allocator::HEAP_START
        )
    } else if prompt_lower.contains("uptime") {
        let secs = crate::rtc::uptime_seconds();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let s = secs % 60;
        alloc::format!("System uptime: {}h {}m {}s", hours, mins, s)
    } else if prompt_lower.contains("process") {
        String::from("Use 'ps' command in the terminal to list running processes.")
    } else if prompt_lower.contains("network") {
        String::from("Network interfaces: lo (127.0.0.1, UP), eth0 (10.0.2.15, DOWN)")
    } else if prompt_lower.contains("knoxos") || prompt_lower.contains("operating system") {
        String::from(
            "KnoxOS is an AI-native operating system written in Rust. \
             It features a graphical desktop environment, Linux-compatible syscalls, \
             and built-in AI inference capabilities.",
        )
    } else {
        alloc::format!(
            "I received your query: '{}'. \
             KnoxOS AI is currently running with limited capabilities. \
             Try asking about 'help', 'cpu', 'memory', 'time', or 'knoxos'.",
            prompt
        )
    }
}
