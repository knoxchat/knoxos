use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════
// PREPROCESSOR
// ═══════════════════════════════════════════════════════════════════════

/// Simple preprocessor for #define, #ifdef, #include
pub struct Preprocessor {
    defines: BTreeMap<String, String>,
    include_paths: Vec<String>,
}

impl Preprocessor {
    pub fn new() -> Self {
        Self {
            defines: BTreeMap::new(),
            include_paths: Vec::new(),
        }
    }

    pub fn define(&mut self, name: &str, value: &str) {
        self.defines.insert(String::from(name), String::from(value));
    }

    pub fn add_include_path(&mut self, path: &str) {
        self.include_paths.push(String::from(path));
    }

    pub fn process(&self, source: &str) -> String {
        let mut output = String::new();
        let mut skip_depth: usize = 0;
        let mut in_ifdef = Vec::new();

        for line in source.lines() {
            let trimmed = line.trim();

            if let Some(rest) = trimmed.strip_prefix("#define ") {
                if skip_depth == 0 {
                    if let Some(_space) = rest.find(' ') {
                        // #define NAME VALUE - handled at definition time
                    }
                }
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("#ifdef ") {
                let name = rest.trim();
                let defined = self.defines.contains_key(name);
                in_ifdef.push(defined);
                if !defined {
                    skip_depth += 1;
                }
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("#ifndef ") {
                let name = rest.trim();
                let defined = self.defines.contains_key(name);
                in_ifdef.push(!defined);
                if defined {
                    skip_depth += 1;
                }
                continue;
            }

            if trimmed == "#else" {
                if let Some(last) = in_ifdef.last_mut() {
                    if *last {
                        skip_depth += 1;
                    } else {
                        skip_depth = skip_depth.saturating_sub(1);
                    }
                    *last = !*last;
                }
                continue;
            }

            if trimmed == "#endif" {
                if let Some(was_active) = in_ifdef.pop() {
                    if !was_active {
                        skip_depth = skip_depth.saturating_sub(1);
                    }
                }
                continue;
            }

            if skip_depth > 0 {
                continue;
            }

            if trimmed.starts_with("#include ") {
                // Include handling - would read file in real implementation
                output.push_str("// included: ");
                output.push_str(trimmed);
                output.push('\n');
                continue;
            }

            // Macro substitution
            let mut processed = String::from(line);
            for (name, value) in &self.defines {
                processed = processed.replace(name.as_str(), value.as_str());
            }
            output.push_str(&processed);
            output.push('\n');
        }

        output
    }
}
