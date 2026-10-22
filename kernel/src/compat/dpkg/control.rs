use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════
// DEBIAN PACKAGE (.deb) FORMAT
// ═══════════════════════════════════════════════════════════════════════

/// Debian package error types
#[derive(Debug)]
pub enum DebError {
    InvalidArchive(&'static str),
    InvalidControl(&'static str),
    MissingMember(&'static str),
    DependencyError(String),
    ConflictError(String),
    InstallError(String),
    UnsupportedCompression(&'static str),
    DecompressError(&'static str),
}

/// Parsed debian control file fields
#[derive(Debug, Clone)]
pub struct DebControl {
    pub package: String,
    pub version: String,
    pub architecture: String,
    pub maintainer: String,
    pub installed_size: u64, // KB
    pub depends: Vec<DebDependency>,
    pub pre_depends: Vec<DebDependency>,
    pub recommends: Vec<DebDependency>,
    pub suggests: Vec<DebDependency>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
    pub provides: Vec<String>,
    pub section: String,
    pub priority: String,
    pub homepage: String,
    pub description: String,
    pub extra_fields: BTreeMap<String, String>,
}

/// A single dependency specification: name (>= version) | alternative
#[derive(Debug, Clone)]
pub struct DebDependency {
    pub package: String,
    pub version_constraint: Option<VersionConstraint>,
    pub alternatives: Vec<DebDependency>,
}

#[derive(Debug, Clone)]
pub struct VersionConstraint {
    pub op: VersionOp,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionOp {
    Eq, // =
    Ge, // >=
    Le, // <=
    Gt, // >>
    Lt, // <<
}

/// Parse a debian control file from text
pub fn parse_control(text: &str) -> Result<DebControl, DebError> {
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut current_key = String::new();
    let mut current_value = String::new();

    for line in text.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line
            if !current_key.is_empty() {
                current_value.push('\n');
                current_value.push_str(line.trim());
            }
        } else if let Some(colon_pos) = line.find(':') {
            // Save previous field
            if !current_key.is_empty() {
                fields.insert(current_key.clone(), current_value.clone());
            }
            current_key = line[..colon_pos].trim().to_string();
            current_value = line[colon_pos + 1..].trim().to_string();
        }
    }
    // Save last field
    if !current_key.is_empty() {
        fields.insert(current_key, current_value);
    }

    let package = fields
        .get("Package")
        .cloned()
        .ok_or(DebError::InvalidControl("missing Package field"))?;
    let version = fields
        .get("Version")
        .cloned()
        .ok_or(DebError::InvalidControl("missing Version field"))?;
    let architecture = fields.get("Architecture").cloned().unwrap_or_default();
    let maintainer = fields.get("Maintainer").cloned().unwrap_or_default();

    let installed_size = fields
        .get("Installed-Size")
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let depends = parse_dependency_list(fields.get("Depends").map(|s| s.as_str()).unwrap_or(""));
    let pre_depends =
        parse_dependency_list(fields.get("Pre-Depends").map(|s| s.as_str()).unwrap_or(""));
    let recommends =
        parse_dependency_list(fields.get("Recommends").map(|s| s.as_str()).unwrap_or(""));
    let suggests = parse_dependency_list(fields.get("Suggests").map(|s| s.as_str()).unwrap_or(""));
    let conflicts = parse_name_list(fields.get("Conflicts").map(|s| s.as_str()).unwrap_or(""));
    let replaces = parse_name_list(fields.get("Replaces").map(|s| s.as_str()).unwrap_or(""));
    let provides = parse_name_list(fields.get("Provides").map(|s| s.as_str()).unwrap_or(""));

    let section = fields.get("Section").cloned().unwrap_or_default();
    let priority = fields.get("Priority").cloned().unwrap_or_default();
    let homepage = fields.get("Homepage").cloned().unwrap_or_default();
    let description = fields.get("Description").cloned().unwrap_or_default();

    Ok(DebControl {
        package,
        version,
        architecture,
        maintainer,
        installed_size,
        depends,
        pre_depends,
        recommends,
        suggests,
        conflicts,
        replaces,
        provides,
        section,
        priority,
        homepage,
        description,
        extra_fields: fields,
    })
}

/// Parse a comma-separated dependency list like "libc6 (>= 2.17), libgcc-s1 (>= 3.0)"
fn parse_dependency_list(text: &str) -> Vec<DebDependency> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    text.split(',')
        .map(|dep_str| {
            let alternatives: Vec<DebDependency> = dep_str
                .split('|')
                .map(|alt| parse_single_dep(alt.trim()))
                .collect();

            if alternatives.len() == 1 {
                alternatives.into_iter().next().unwrap()
            } else {
                let first = alternatives[0].clone();
                DebDependency {
                    package: first.package,
                    version_constraint: first.version_constraint,
                    alternatives: alternatives[1..].to_vec(),
                }
            }
        })
        .collect()
}

/// Parse a single dependency like "libc6 (>= 2.17)"
fn parse_single_dep(text: &str) -> DebDependency {
    let text = text.trim();

    if let Some(paren_start) = text.find('(') {
        let package = text[..paren_start].trim().to_string();
        let constraint_str = text[paren_start + 1..].trim_end_matches(')').trim();

        let version_constraint = parse_version_constraint(constraint_str);

        DebDependency {
            package,
            version_constraint,
            alternatives: Vec::new(),
        }
    } else {
        DebDependency {
            package: text.to_string(),
            version_constraint: None,
            alternatives: Vec::new(),
        }
    }
}

fn parse_version_constraint(text: &str) -> Option<VersionConstraint> {
    let (op, rest) = if let Some(rest) = text.strip_prefix(">=") {
        (VersionOp::Ge, rest.trim())
    } else if let Some(rest) = text.strip_prefix("<=") {
        (VersionOp::Le, rest.trim())
    } else if let Some(rest) = text.strip_prefix(">>") {
        (VersionOp::Gt, rest.trim())
    } else if let Some(rest) = text.strip_prefix("<<") {
        (VersionOp::Lt, rest.trim())
    } else if let Some(rest) = text.strip_prefix('=') {
        (VersionOp::Eq, rest.trim())
    } else {
        return None;
    };

    Some(VersionConstraint {
        op,
        version: rest.to_string(),
    })
}

fn parse_name_list(text: &str) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    text.split(',').map(|s| s.trim().to_string()).collect()
}
