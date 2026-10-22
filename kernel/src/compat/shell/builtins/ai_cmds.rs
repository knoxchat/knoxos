/// AI / LLM shell commands — natural language to command translation,
/// tensor operations, model management, and AI query interface.
///
/// Commands:
///   ask <question>        — Query the AI assistant
///   ai query <prompt>     — Same as ask
///   ai model list         — List loaded AI/LLM models
///   ai model load <type>  — Load a model (classifier, sentiment, etc.)
///   ai model unload <id>  — Unload a model
///   ai infer <id> <data>  — Run inference on a loaded model
///   ai simd               — Show SIMD capabilities
///   ai suggest <task>     — Suggest shell commands for a task
///   llm load              — Load LLM (tiny test model)
///   llm generate <prompt> — Generate text
///   llm models            — List LLM models
///   tensor info           — Show tensor engine info
///   tensor bench          — Run SIMD benchmark
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::shell::types::ShellResult;

/// Main dispatch for ai/ask/llm/tensor commands
pub fn dispatch_ai(program: &str, args: &[String]) -> ShellResult {
    match program {
        "ask" => cmd_ask(args),
        "ai" => cmd_ai(args),
        "llm" => cmd_llm(args),
        "tensor" => cmd_tensor(args),
        _ => ShellResult::err("Unknown AI command"),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ask — Natural language query
// ═══════════════════════════════════════════════════════════════════════

fn cmd_ask(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("Usage: ask <question>");
    }

    let prompt = args.join(" ");
    let response = crate::ai::query(&prompt);
    ShellResult::ok(&response)
}

// ═══════════════════════════════════════════════════════════════════════
// ai — AI subsystem commands
// ═══════════════════════════════════════════════════════════════════════

fn cmd_ai(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::ok(
            "KnoxOS AI Subsystem\n\
             Usage:\n\
             \x20 ai query <prompt>      — Ask the AI assistant\n\
             \x20 ai suggest <task>      — Suggest shell commands for a task\n\
             \x20 ai model list          — List loaded models\n\
             \x20 ai model load <type>   — Load a model\n\
             \x20 ai model unload <id>   — Unload a model\n\
             \x20 ai infer <id> <data>   — Run inference\n\
             \x20 ai simd               — Show SIMD capabilities\n\
             \x20 ai bench              — Run SIMD benchmark\n",
        );
    }

    let sub = args[0].as_str();
    let sub_args = &args[1..];

    match sub {
        "query" => cmd_ask(sub_args),
        "suggest" => cmd_suggest(sub_args),
        "model" => cmd_model(sub_args),
        "infer" => cmd_infer(sub_args),
        "simd" => cmd_simd_info(),
        "bench" => cmd_bench(),
        _ => ShellResult::err(&format!("Unknown ai subcommand: {}", sub)),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ai suggest — Natural language to shell command translation
// ═══════════════════════════════════════════════════════════════════════

fn cmd_suggest(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("Usage: ai suggest <task description>");
    }

    let task = args.join(" ").to_ascii_lowercase();
    let suggestion = suggest_command(&task);

    ShellResult::ok(&format!(
        "Suggested command for '{}':\n\n  {}\n",
        args.join(" "),
        suggestion
    ))
}

/// Map natural language task descriptions to shell commands
fn suggest_command(task: &str) -> &'static str {
    // Pattern-match common tasks to shell commands
    if task.contains("list") && task.contains("file") {
        "ls -la"
    } else if task.contains("find") && task.contains("file") {
        "find / -name '<pattern>' -type f"
    } else if task.contains("disk") && (task.contains("space") || task.contains("usage")) {
        "df -h"
    } else if task.contains("memory") && (task.contains("usage") || task.contains("free")) {
        "free -h"
    } else if task.contains("process") && (task.contains("list") || task.contains("running")) {
        "ps aux"
    } else if task.contains("kill") && task.contains("process") {
        "kill -9 <pid>"
    } else if task.contains("network") && task.contains("interface") {
        "ip addr show"
    } else if task.contains("network") && task.contains("connect") {
        "ping <host>"
    } else if task.contains("dns") || task.contains("resolve") {
        "dig <domain>"
    } else if task.contains("install") && task.contains("package") {
        "apt install <package>"
    } else if task.contains("update") && task.contains("package") {
        "apt update && apt upgrade"
    } else if task.contains("search") && task.contains("package") {
        "apt search <query>"
    } else if task.contains("create") && task.contains("directory") {
        "mkdir -p <path>"
    } else if task.contains("copy") && task.contains("file") {
        "cp <source> <destination>"
    } else if task.contains("move") || task.contains("rename") {
        "mv <source> <destination>"
    } else if task.contains("delete") || task.contains("remove") {
        "rm <file>"
    } else if task.contains("edit") || task.contains("text editor") {
        "nano <file>"
    } else if task.contains("compress") || task.contains("zip") || task.contains("archive") {
        "tar -czf archive.tar.gz <files>"
    } else if task.contains("extract") || task.contains("decompress") || task.contains("unzip") {
        "tar -xzf archive.tar.gz"
    } else if task.contains("permission") || task.contains("chmod") {
        "chmod 755 <file>"
    } else if task.contains("owner") || task.contains("chown") {
        "chown user:group <file>"
    } else if task.contains("search") && task.contains("text") {
        "grep -r '<pattern>' <directory>"
    } else if task.contains("count") && task.contains("line") {
        "wc -l <file>"
    } else if task.contains("sort") {
        "sort <file>"
    } else if task.contains("download") {
        "wget <url>"
    } else if task.contains("cpu") && task.contains("info") {
        "lscpu"
    } else if task.contains("kernel") && task.contains("version") {
        "uname -r"
    } else if task.contains("date") || task.contains("time") {
        "date"
    } else if task.contains("uptime") {
        "uptime"
    } else if task.contains("mount") {
        "mount"
    } else if task.contains("who") && task.contains("logged") {
        "who"
    } else if task.contains("system") && task.contains("info") {
        "neofetch"
    } else if task.contains("port") && task.contains("listen") {
        "ss -tulpn"
    } else if task.contains("firewall") {
        "iptables -L -n"
    } else if task.contains("service") && task.contains("status") {
        "systemctl status <service>"
    } else if task.contains("log") {
        "dmesg | tail -50"
    } else if task.contains("shutdown") || task.contains("power off") {
        "poweroff"
    } else if task.contains("reboot") || task.contains("restart") {
        "reboot"
    } else {
        "# No specific suggestion available. Try: ask <your question>"
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ai model — Model management
// ═══════════════════════════════════════════════════════════════════════

fn cmd_model(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("Usage: ai model <list|load|unload> [args]");
    }

    match args[0].as_str() {
        "list" => {
            let models = crate::ai::list_models();
            let llm_models = crate::llm::list_models();

            let mut out = String::from("Loaded Models:\n");
            out.push_str("\n  AI/NN Models:\n");
            if models.is_empty() {
                out.push_str("    (none)\n");
            }
            for (id, name) in &models {
                out.push_str(&format!("    [{}] {}\n", id, name));
            }

            out.push_str("\n  LLM Models:\n");
            if llm_models.is_empty() {
                out.push_str("    (none)\n");
            }
            for (id, name, mem) in &llm_models {
                out.push_str(&format!(
                    "    [{}] {} ({} MB)\n",
                    id,
                    name,
                    mem / (1024 * 1024)
                ));
            }

            ShellResult::ok(&out)
        }
        "load" => {
            let model_type = if args.len() > 1 {
                args[1].as_str()
            } else {
                "default"
            };
            let id = crate::ai::load_model(model_type);
            ShellResult::ok(&format!("Model '{}' loaded with ID {}", model_type, id))
        }
        "unload" => {
            if args.len() < 2 {
                return ShellResult::err("Usage: ai model unload <id>");
            }
            let id: u64 = args[1].parse().unwrap_or(0);
            if crate::ai::unload_model(id) {
                ShellResult::ok(&format!("Model {} unloaded", id))
            } else {
                ShellResult::err(&format!("Model {} not found", id))
            }
        }
        _ => ShellResult::err("Usage: ai model <list|load|unload>"),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ai infer — Run inference
// ═══════════════════════════════════════════════════════════════════════

fn cmd_infer(args: &[String]) -> ShellResult {
    if args.len() < 2 {
        return ShellResult::err("Usage: ai infer <model_id> <input_values...>");
    }

    let model_id: u64 = args[0].parse().unwrap_or(0);
    let input: Vec<f32> = args[1..]
        .iter()
        .map(|s| s.parse::<f32>().unwrap_or(0.0))
        .collect();

    match crate::ai::infer(model_id, &input) {
        Ok(output) => {
            let argmax = output
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(0);

            let mut out = String::from("Inference result:\n");
            for (i, &val) in output.iter().enumerate() {
                let marker = if i == argmax { " ← max" } else { "" };
                out.push_str(&format!("  [{}] {:.6}{}\n", i, val, marker));
            }
            ShellResult::ok(&out)
        }
        Err(e) => ShellResult::err(&format!("Inference error: {}", e)),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ai simd — SIMD capabilities
// ═══════════════════════════════════════════════════════════════════════

fn cmd_simd_info() -> ShellResult {
    let level = crate::ai::simd::detect();
    let mut out = String::from("SIMD Capabilities:\n");
    out.push_str(&format!("  Detected level: {:?}\n", level));
    out.push_str("\n  Available operations:\n");
    out.push_str("    dot_product  — Vector dot product\n");
    out.push_str("    vec_add      — Vector addition\n");
    out.push_str("    vec_mul      — Vector element-wise multiply\n");
    out.push_str("    vec_scale    — Vector scalar multiply\n");
    out.push_str("    matmul_simd  — Matrix multiplication\n");
    out.push_str("    relu_simd    — ReLU activation\n");
    out.push_str("    softmax_simd — Softmax\n");
    out.push_str("    layer_norm   — Layer normalization\n");
    out.push_str("    rms_norm     — RMS normalization (LLaMA)\n");
    ShellResult::ok(&out)
}

// ═══════════════════════════════════════════════════════════════════════
// ai bench — SIMD benchmark
// ═══════════════════════════════════════════════════════════════════════

fn cmd_bench() -> ShellResult {
    let n = 1024;
    let a: Vec<f32> = (0..n).map(|i| (i as f32) * 0.001).collect();
    let b: Vec<f32> = (0..n).map(|i| ((n - i) as f32) * 0.001).collect();

    let start = crate::clock::get_ticks();

    // Dot product benchmark
    let mut _dot = 0.0f32;
    for _ in 0..1000 {
        _dot += crate::ai::simd::dot_product(&a, &b);
    }
    let dot_ticks = crate::clock::get_ticks() - start;

    // MatMul benchmark (32x32)
    let m = 32;
    let k = 32;
    let nn = 32;
    let mat_a: Vec<f32> = (0..m * k).map(|i| (i as f32) * 0.01).collect();
    let mat_b: Vec<f32> = (0..k * nn).map(|i| (i as f32) * 0.01).collect();

    let start2 = crate::clock::get_ticks();
    for _ in 0..100 {
        let _ = crate::ai::simd::matmul_simd(&mat_a, &mat_b, m, k, nn);
    }
    let matmul_ticks = crate::clock::get_ticks() - start2;

    // ReLU benchmark
    let start3 = crate::clock::get_ticks();
    for _ in 0..1000 {
        let _ = crate::ai::simd::relu_simd(&a);
    }
    let relu_ticks = crate::clock::get_ticks() - start3;

    let level = crate::ai::simd::detect();
    let mut out = format!("SIMD Benchmark (level: {:?})\n", level);
    out.push_str(&format!(
        "  dot_product (1024-dim, 1000 iters): {} ticks\n",
        dot_ticks
    ));
    out.push_str(&format!(
        "  matmul (32x32, 100 iters):          {} ticks\n",
        matmul_ticks
    ));
    out.push_str(&format!(
        "  relu (1024-dim, 1000 iters):        {} ticks\n",
        relu_ticks
    ));
    ShellResult::ok(&out)
}

// ═══════════════════════════════════════════════════════════════════════
// llm — LLM commands
// ═══════════════════════════════════════════════════════════════════════

fn cmd_llm(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::ok(
            "KnoxOS LLM Engine\n\
             Usage:\n\
             \x20 llm load [model]       — Load a language model\n\
             \x20 llm generate <prompt>  — Generate text\n\
             \x20 llm models             — List loaded models\n\
             \x20 llm unload <id>        — Unload a model\n\
             \x20 llm info <id>          — Show model info\n",
        );
    }

    match args[0].as_str() {
        "load" => {
            let config = if args.len() > 1 && args[1] == "7b" {
                crate::llm::ModelConfig::llama_7b()
            } else {
                crate::llm::ModelConfig::tiny()
            };
            let name = config.name.clone();
            match crate::llm::load_model(config) {
                Ok(id) => ShellResult::ok(&format!("LLM '{}' loaded (id={})", name, id)),
                Err(e) => ShellResult::err(&format!("Failed to load LLM: {}", e)),
            }
        }
        "generate" => {
            if args.len() < 2 {
                return ShellResult::err("Usage: llm generate <prompt>");
            }
            let prompt = args[1..].join(" ");

            // Use the first loaded model
            let models = crate::llm::list_models();
            let model_id = if let Some((id, _, _)) = models.first() {
                *id
            } else {
                // Auto-load a tiny model
                match crate::llm::load_model(crate::llm::ModelConfig::tiny()) {
                    Ok(id) => id,
                    Err(e) => return ShellResult::err(&format!("No model loaded: {}", e)),
                }
            };

            let params = crate::llm::SamplingParams::default();
            match crate::llm::generate(model_id, &prompt, &params) {
                Ok(text) => ShellResult::ok(&format!("> {}\n\n{}", prompt, text)),
                Err(e) => ShellResult::err(&format!("Generation error: {}", e)),
            }
        }
        "models" => {
            let models = crate::llm::list_models();
            if models.is_empty() {
                return ShellResult::ok("No LLM models loaded. Use 'llm load' to load one.");
            }
            let mut out = String::from("Loaded LLM models:\n");
            for (id, name, mem) in &models {
                out.push_str(&format!(
                    "  [{}] {} ({} MB)\n",
                    id,
                    name,
                    mem / (1024 * 1024)
                ));
            }
            ShellResult::ok(&out)
        }
        "unload" => {
            if args.len() < 2 {
                return ShellResult::err("Usage: llm unload <id>");
            }
            let id: u32 = args[1].parse().unwrap_or(0);
            match crate::llm::unload_model(id) {
                Ok(()) => ShellResult::ok(&format!("LLM {} unloaded", id)),
                Err(e) => ShellResult::err(&format!("Failed: {}", e)),
            }
        }
        "info" => {
            if args.len() < 2 {
                return ShellResult::err("Usage: llm info <id>");
            }
            let id: u32 = args[1].parse().unwrap_or(0);
            match crate::llm::model_info(id) {
                Ok(config) => {
                    let out = format!(
                        "Model: {}\n\
                         Architecture: {}\n\
                         Vocab: {}\n\
                         Hidden size: {}\n\
                         Layers: {}\n\
                         Heads: {} (KV: {})\n\
                         Max seq len: {}\n\
                         Dtype: {:?}\n\
                         Est. memory: {} MB\n",
                        config.name,
                        config.architecture,
                        config.vocab_size,
                        config.hidden_size,
                        config.num_layers,
                        config.num_heads,
                        config.num_kv_heads,
                        config.max_seq_len,
                        config.dtype,
                        config.estimated_memory() / (1024 * 1024)
                    );
                    ShellResult::ok(&out)
                }
                Err(e) => ShellResult::err(&format!("Failed: {}", e)),
            }
        }
        _ => ShellResult::err(&format!("Unknown llm subcommand: {}", args[0])),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// tensor — Tensor operation commands
// ═══════════════════════════════════════════════════════════════════════

fn cmd_tensor(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::ok(
            "KnoxOS Tensor Engine\n\
             Usage:\n\
             \x20 tensor info   — Show tensor engine capabilities\n\
             \x20 tensor bench  — Run SIMD benchmark\n\
             \x20 tensor demo   — Run a small neural network demo\n",
        );
    }

    match args[0].as_str() {
        "info" => cmd_simd_info(),
        "bench" => cmd_bench(),
        "demo" => cmd_tensor_demo(),
        _ => ShellResult::err(&format!("Unknown tensor subcommand: {}", args[0])),
    }
}

fn cmd_tensor_demo() -> ShellResult {
    use crate::ai::{NeuralNetwork, Tensor};

    let mut out = String::from("Neural Network Demo\n");
    out.push_str("===================\n\n");

    // Create a small network: 4 → 8 → 3
    let mut nn = NeuralNetwork::new("demo-net");
    nn.add_linear(4, 8);
    nn.add_activation("relu");
    nn.add_linear(8, 3);
    nn.add_activation("softmax");

    out.push_str("Network: 4 → 8 (ReLU) → 3 (Softmax)\n\n");

    // Run inference with test data
    let input = Tensor::from_data(vec![1.0, 0.5, -0.3, 0.8], &[1, 4]).unwrap();
    match nn.forward(&input) {
        Ok(output) => {
            out.push_str("Input:  [1.0, 0.5, -0.3, 0.8]\n");
            out.push_str("Output: [");
            for (i, &v) in output.data.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format!("{:.4}", v));
            }
            out.push_str("]\n");

            let class = output.argmax();
            out.push_str(&format!("Predicted class: {}\n", class));
        }
        Err(e) => {
            out.push_str(&format!("Error: {}\n", e));
        }
    }

    ShellResult::ok(&out)
}
