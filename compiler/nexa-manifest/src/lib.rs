use std::fmt;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("NEXA-MANIFEST-0001: Invalid package name '{name}': must be ASCII lowercase with . or - separators")]
    InvalidPackageName { name: String },
    #[error(
        "NEXA-MANIFEST-0002: Invalid version '{version}': must be SemVer-like MAJOR.MINOR.PATCH"
    )]
    InvalidVersion { version: String },
    #[error("NEXA-MANIFEST-0003: Invalid target kind '{kind}': only 'executable' and 'library' are supported")]
    InvalidTargetKind { kind: String },
    #[error("NEXA-MANIFEST-0004: Invalid version requirement '{req}'")]
    InvalidVersionRequirement { req: String },
    #[error("NEXA-MANIFEST-0005: Duplicate target name '{name}'")]
    DuplicateTarget { name: String },
    #[error("NEXA-MANIFEST-0006: Dependency alias '{alias}' is reserved or invalid")]
    InvalidAlias { alias: String },
    #[error("NEXA-MANIFEST-0007: Path dependency must have a path")]
    PathDependencyMissingPath,
    #[error("NEXA-MANIFEST-0008: Registry dependency '{alias}' must have a package name")]
    RegistryDependencyMissingPackage { alias: String },
    #[error("NEXA-MANIFEST-0009: Missing required project metadata field '{field}'")]
    MissingField { field: String },
    #[error("NEXA-MANIFEST-0010: Manifest file could not be parsed")]
    ParseError,
    #[error("NEXA-MANIFEST-0011: Entry module '{entry}' for target '{target}' not found in src/")]
    EntryNotFound { target: String, entry: String },
}

#[derive(Debug, Clone)]
pub struct PackageName(String);

impl PackageName {
    pub fn new(name: &str) -> Result<Self, ManifestError> {
        if name.is_empty() {
            return Err(ManifestError::InvalidPackageName {
                name: name.to_string(),
            });
        }

        let mut prev_was_separator = false;

        for ch in name.chars() {
            match ch {
                'a'..='z' | '0'..='9' => {
                    prev_was_separator = false;
                }
                '.' | '-' => {
                    if prev_was_separator || name.starts_with(ch) || name.ends_with(ch) {
                        return Err(ManifestError::InvalidPackageName {
                            name: name.to_string(),
                        });
                    }
                    prev_was_separator = true;
                }
                _ => {
                    return Err(ManifestError::InvalidPackageName {
                        name: name.to_string(),
                    });
                }
            }
        }

        Ok(PackageName(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct SemanticVersion {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Option<String>,
    build_metadata: Option<String>,
}

impl SemanticVersion {
    pub fn new(version: &str) -> Result<Self, ManifestError> {
        let (version_part, build_metadata) = match version.split_once('+') {
            Some((v, b)) => (v, Some(b.to_string())),
            None => (version, None),
        };

        let (version_part, prerelease) = match version_part.split_once('-') {
            Some((v, p)) => (v, Some(p.to_string())),
            None => (version_part, None),
        };

        let parts: Vec<&str> = version_part.split('.').collect();
        if parts.len() != 3 {
            return Err(ManifestError::InvalidVersion {
                version: version.to_string(),
            });
        }

        let major = parts[0]
            .parse::<u64>()
            .map_err(|_| ManifestError::InvalidVersion {
                version: version.to_string(),
            })?;
        let minor = parts[1]
            .parse::<u64>()
            .map_err(|_| ManifestError::InvalidVersion {
                version: version.to_string(),
            })?;
        let patch = parts[2]
            .parse::<u64>()
            .map_err(|_| ManifestError::InvalidVersion {
                version: version.to_string(),
            })?;

        Ok(SemanticVersion {
            major,
            minor,
            patch,
            prerelease,
            build_metadata,
        })
    }

    pub fn is_compatible_with(&self, req: &VersionRequirement) -> bool {
        req.matches(self)
    }

    fn cmp_pre_release(a: &Option<String>, b: &Option<String>) -> std::cmp::Ordering {
        match (a, b) {
            (None, None) => std::cmp::Ordering::Equal,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

impl Ord for SemanticVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then(Self::cmp_pre_release(&self.prerelease, &other.prerelease))
    }
}

impl PartialOrd for SemanticVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for SemanticVersion {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major && self.minor == other.minor && self.patch == other.patch
    }
}

impl Eq for SemanticVersion {}

impl fmt::Display for SemanticVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pre) = self.prerelease {
            write!(f, "-{}", pre)?;
        }
        if let Some(ref build) = self.build_metadata {
            write!(f, "+{}", build)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct VersionRequirement(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionReqOp {
    Caret,
    Tilde,
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Wildcard,
}

#[derive(Debug, Clone)]
struct ParsedReq {
    op: VersionReqOp,
    major: Option<u64>,
    minor: Option<u64>,
    patch: Option<u64>,
}

fn parse_version_req(req: &str) -> Result<ParsedReq, ManifestError> {
    let req = req.trim();

    if req == "*" || req == "x" || req == "X" {
        return Ok(ParsedReq {
            op: VersionReqOp::Wildcard,
            major: None,
            minor: None,
            patch: None,
        });
    }

    if let Some(rest) = req.strip_prefix("^") {
        let parts = parse_version_parts(rest)?;
        Ok(ParsedReq {
            op: VersionReqOp::Caret,
            major: parts.0,
            minor: parts.1,
            patch: parts.2,
        })
    } else if let Some(rest) = req.strip_prefix("~") {
        let parts = parse_version_parts(rest)?;
        Ok(ParsedReq {
            op: VersionReqOp::Tilde,
            major: parts.0,
            minor: parts.1,
            patch: parts.2,
        })
    } else if let Some(rest) = req.strip_prefix(">=") {
        let parts = parse_version_parts(rest)?;
        Ok(ParsedReq {
            op: VersionReqOp::Gte,
            major: parts.0,
            minor: parts.1,
            patch: parts.2,
        })
    } else if let Some(rest) = req.strip_prefix(">") {
        let parts = parse_version_parts(rest)?;
        Ok(ParsedReq {
            op: VersionReqOp::Gt,
            major: parts.0,
            minor: parts.1,
            patch: parts.2,
        })
    } else if let Some(rest) = req.strip_prefix("<=") {
        let parts = parse_version_parts(rest)?;
        Ok(ParsedReq {
            op: VersionReqOp::Lte,
            major: parts.0,
            minor: parts.1,
            patch: parts.2,
        })
    } else if let Some(rest) = req.strip_prefix("<") {
        let parts = parse_version_parts(rest)?;
        Ok(ParsedReq {
            op: VersionReqOp::Lt,
            major: parts.0,
            minor: parts.1,
            patch: parts.2,
        })
    } else if let Some(rest) = req.strip_prefix("=") {
        let parts = parse_version_parts(rest)?;
        Ok(ParsedReq {
            op: VersionReqOp::Eq,
            major: parts.0,
            minor: parts.1,
            patch: parts.2,
        })
    } else {
        let parts = parse_version_parts(req)?;
        if parts.2.is_none() && parts.1.is_none() {
            let wildcard_part = req.split('.').nth(1).unwrap_or("");
            if wildcard_part == "x" || wildcard_part == "X" || wildcard_part == "*" {
                return Ok(ParsedReq {
                    op: VersionReqOp::Caret,
                    major: parts.0,
                    minor: None,
                    patch: None,
                });
            }
        }
        Ok(ParsedReq {
            op: VersionReqOp::Eq,
            major: parts.0,
            minor: parts.1,
            patch: parts.2,
        })
    }
}

#[allow(clippy::type_complexity)]
fn parse_version_parts(s: &str) -> Result<(Option<u64>, Option<u64>, Option<u64>), ManifestError> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(ManifestError::InvalidVersionRequirement { req: s.to_string() });
    }

    let parse_part = |p: &str| -> Result<Option<u64>, ManifestError> {
        if p == "x" || p == "X" || p == "*" {
            Ok(None)
        } else {
            p.parse::<u64>()
                .map(Some)
                .map_err(|_| ManifestError::InvalidVersionRequirement { req: s.to_string() })
        }
    };

    let major = parse_part(parts[0])?;
    let minor = if parts.len() > 1 {
        parse_part(parts[1])?
    } else {
        None
    };
    let patch = if parts.len() > 2 {
        parse_part(parts[2])?
    } else {
        None
    };

    Ok((major, minor, patch))
}

impl VersionRequirement {
    pub fn new(req: &str) -> Result<Self, ManifestError> {
        parse_version_req(req)?;
        Ok(VersionRequirement(req.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn matches(&self, version: &SemanticVersion) -> bool {
        let parsed = match parse_version_req(&self.0) {
            Ok(p) => p,
            Err(_) => return false,
        };

        match parsed.op {
            VersionReqOp::Wildcard => true,
            VersionReqOp::Eq => match_major_minor_patch(&parsed, version, |v, m, mi, p| {
                v.major == m && v.minor == mi && v.patch == p
            }),
            VersionReqOp::Gte => match_major_minor_patch(&parsed, version, |_v, m, mi, p| {
                let req = SemanticVersion {
                    major: m,
                    minor: mi,
                    patch: p,
                    prerelease: None,
                    build_metadata: None,
                };
                *version >= req
            }),
            VersionReqOp::Gt => match_major_minor_patch(&parsed, version, |_v, m, mi, p| {
                let req = SemanticVersion {
                    major: m,
                    minor: mi,
                    patch: p,
                    prerelease: None,
                    build_metadata: None,
                };
                *version > req
            }),
            VersionReqOp::Lte => match_major_minor_patch(&parsed, version, |_v, m, mi, p| {
                let req = SemanticVersion {
                    major: m,
                    minor: mi,
                    patch: p,
                    prerelease: None,
                    build_metadata: None,
                };
                *version <= req
            }),
            VersionReqOp::Lt => match_major_minor_patch(&parsed, version, |_v, m, mi, p| {
                let req = SemanticVersion {
                    major: m,
                    minor: mi,
                    patch: p,
                    prerelease: None,
                    build_metadata: None,
                };
                *version < req
            }),
            VersionReqOp::Caret => {
                let major = parsed.major.unwrap_or(0);
                let minor = parsed.minor.unwrap_or(0);
                let patch = parsed.patch.unwrap_or(0);

                let lower = SemanticVersion {
                    major,
                    minor,
                    patch,
                    prerelease: None,
                    build_metadata: None,
                };

                if *version < lower {
                    return false;
                }

                if major == 0 {
                    if minor == 0 {
                        version.major == 0 && version.minor == 0 && version.patch == patch
                    } else {
                        version.major == 0 && version.minor == minor && version.patch >= patch
                    }
                } else {
                    version.major == major
                        && (version.minor > minor
                            || (version.minor == minor && version.patch >= patch))
                }
            }
            VersionReqOp::Tilde => {
                let major = parsed.major.unwrap_or(0);
                let minor = parsed.minor.unwrap_or(0);
                let patch = parsed.patch.unwrap_or(0);

                let lower = SemanticVersion {
                    major,
                    minor,
                    patch,
                    prerelease: None,
                    build_metadata: None,
                };

                if *version < lower {
                    return false;
                }

                version.major == major && version.minor == minor
            }
        }
    }
}

fn match_major_minor_patch<F>(parsed: &ParsedReq, version: &SemanticVersion, f: F) -> bool
where
    F: FnOnce(&SemanticVersion, u64, u64, u64) -> bool,
{
    let major = match parsed.major {
        Some(m) => m,
        None => version.major,
    };
    let minor = match parsed.minor {
        Some(m) => m,
        None => version.minor,
    };
    let patch = match parsed.patch {
        Some(p) => p,
        None => version.patch,
    };
    f(version, major, minor, patch)
}

#[derive(Debug, Clone)]
pub struct LanguageVersionRequirement(String);

impl LanguageVersionRequirement {
    pub fn new(version: &str) -> Result<Self, ManifestError> {
        Ok(LanguageVersionRequirement(version.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct TargetName(String);

impl TargetName {
    pub fn new(name: &str) -> Result<Self, ManifestError> {
        Ok(TargetName(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct ModulePath(String);

impl ModulePath {
    pub fn new(path: &str) -> Result<Self, ManifestError> {
        Ok(ModulePath(path.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Executable,
    Library,
}

impl TargetKind {
    pub fn parse(s: &str) -> Result<Self, ManifestError> {
        match s {
            "executable" => Ok(TargetKind::Executable),
            "library" => Ok(TargetKind::Library),
            _ => Err(ManifestError::InvalidTargetKind {
                kind: s.to_string(),
            }),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TargetKind::Executable => "executable",
            TargetKind::Library => "library",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DependencyAlias(String);

impl DependencyAlias {
    pub fn new(name: &str) -> Result<Self, ManifestError> {
        Ok(DependencyAlias(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct PackageNameRef(String);

impl PackageNameRef {
    pub fn new(name: &str) -> Result<Self, ManifestError> {
        Ok(PackageNameRef(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct PathRef(String);

impl PathRef {
    pub fn new(path: &str) -> Result<Self, ManifestError> {
        Ok(PathRef(path.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencySource {
    Registry,
    Path,
}

#[derive(Debug, Clone)]
pub struct ProjectMetadata {
    pub name: PackageName,
    pub version: SemanticVersion,
    pub language: LanguageVersionRequirement,
}

#[derive(Debug, Clone)]
pub struct TargetManifest {
    pub name: TargetName,
    pub kind: TargetKind,
    pub entry: Option<ModulePath>,
    pub public_modules: Vec<ModulePath>,
}

#[derive(Debug, Clone)]
pub struct CapabilityManifest {
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DependencyManifest {
    pub alias: DependencyAlias,
    pub package: Option<PackageNameRef>,
    pub path: Option<PathRef>,
    pub version_req: Option<VersionRequirement>,
    pub source: DependencySource,
}

#[derive(Debug, Clone)]
pub struct ProjectManifest {
    pub project: ProjectMetadata,
    pub targets: Vec<TargetManifest>,
    pub dependencies: Vec<DependencyManifest>,
    pub dev_dependencies: Vec<DependencyManifest>,
}

struct ManifestParser {
    lines: Vec<String>,
    pos: usize,
}

impl ManifestParser {
    fn new(content: &str) -> Self {
        let lines: Vec<String> = content.lines().map(Self::strip_comment).collect();
        ManifestParser { lines, pos: 0 }
    }

    fn strip_comment(line: &str) -> String {
        let mut in_string = false;
        let mut chars = line.char_indices();
        while let Some((i, ch)) = chars.next() {
            match ch {
                '"' => {
                    in_string = !in_string;
                }
                '/' if !in_string => {
                    if let Some((_j, '/')) = chars.next() {
                        return line[..i].to_string();
                    }
                }
                _ => {}
            }
        }
        line.to_string()
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.lines.len() {
            let line = self.lines[self.pos].trim();
            if line.is_empty() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn current_line(&self) -> Option<&str> {
        if self.pos < self.lines.len() {
            Some(self.lines[self.pos].trim())
        } else {
            None
        }
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn parse_block_header(&mut self) -> Result<(String, String), ManifestError> {
        let line = self.current_line().ok_or(ManifestError::ParseError)?;

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 || parts.last() != Some(&"{") {
            return Err(ManifestError::ParseError);
        }

        let block_type = parts[0].to_string();
        let raw_name = parts[1..parts.len() - 1].join(" ");
        let block_name = Self::parse_string_value(&raw_name).unwrap_or(raw_name);
        self.advance();
        Ok((block_type, block_name))
    }

    fn parse_block_entries(&mut self) -> Result<Vec<(String, String)>, ManifestError> {
        let mut entries = Vec::new();
        loop {
            self.skip_whitespace();
            let line = self.current_line().ok_or(ManifestError::ParseError)?;
            if line == "}" {
                self.advance();
                break;
            }

            if line.contains('{') && !line.contains('=') {
                return Err(ManifestError::ParseError);
            }

            let eq_pos = line.find('=').ok_or(ManifestError::ParseError)?;
            let key = line[..eq_pos].trim().to_string();
            let value = line[eq_pos + 1..].trim().to_string();
            entries.push((key, value));
            self.advance();
        }
        Ok(entries)
    }

    fn parse_array_value(value: &str) -> Result<Vec<String>, ManifestError> {
        let value = value.trim();
        if !value.starts_with('[') || !value.ends_with(']') {
            return Err(ManifestError::ParseError);
        }
        let inner = &value[1..value.len() - 1];
        if inner.trim().is_empty() {
            return Ok(Vec::new());
        }
        let items: Result<Vec<String>, ManifestError> = inner
            .split(',')
            .map(|item| {
                let item = item.trim();
                Self::parse_string_value(item)
            })
            .collect();
        items
    }

    fn parse_string_value(value: &str) -> Result<String, ManifestError> {
        let value = value.trim();
        if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
            Ok(value[1..value.len() - 1].to_string())
        } else {
            Err(ManifestError::ParseError)
        }
    }
}

impl ProjectManifest {
    pub fn parse(content: &str) -> Result<Self, ManifestError> {
        let mut parser = ManifestParser::new(content);

        let mut project_metadata = None;
        let mut targets = Vec::new();
        let mut dependencies = Vec::new();
        let mut dev_dependencies = Vec::new();

        let mut project_count = 0;

        loop {
            parser.skip_whitespace();
            if parser.current_line().is_none() {
                break;
            }

            let line = parser.current_line().unwrap();
            if line == "}" {
                parser.advance();
                continue;
            }

            if line.starts_with("//") {
                parser.advance();
                continue;
            }

            let (block_type, block_name) = parser.parse_block_header()?;
            let entries = parser.parse_block_entries()?;

            match block_type.as_str() {
                "project" => {
                    project_count += 1;
                    if project_count > 1 {
                        return Err(ManifestError::ParseError);
                    }

                    let mut name = None;
                    let mut version = None;
                    let mut language = None;

                    for (key, value) in &entries {
                        match key.as_str() {
                            "name" => {
                                let s = ManifestParser::parse_string_value(value)?;
                                name = Some(PackageName::new(&s)?);
                            }
                            "version" => {
                                let s = ManifestParser::parse_string_value(value)?;
                                version = Some(SemanticVersion::new(&s)?);
                            }
                            "language" => {
                                let s = ManifestParser::parse_string_value(value)?;
                                language = Some(LanguageVersionRequirement::new(&s)?);
                            }
                            _ => {}
                        }
                    }

                    let name = name.ok_or_else(|| ManifestError::MissingField {
                        field: "name".to_string(),
                    })?;
                    let version = version.ok_or_else(|| ManifestError::MissingField {
                        field: "version".to_string(),
                    })?;
                    let language = language.ok_or_else(|| ManifestError::MissingField {
                        field: "language".to_string(),
                    })?;

                    project_metadata = Some(ProjectMetadata {
                        name,
                        version,
                        language,
                    });
                }
                "target" => {
                    let target_name = TargetName::new(&block_name)?;
                    let mut kind = None;
                    let mut entry = None;
                    let mut public_modules = Vec::new();

                    for (key, value) in &entries {
                        match key.as_str() {
                            "kind" => {
                                let s = ManifestParser::parse_string_value(value)?;
                                kind = Some(TargetKind::parse(&s)?);
                            }
                            "entry" => {
                                let s = ManifestParser::parse_string_value(value)?;
                                entry = Some(ModulePath::new(&s)?);
                            }
                            "publicModules" => {
                                let arr = ManifestParser::parse_array_value(value)?;
                                public_modules = arr
                                    .into_iter()
                                    .map(|s| ModulePath::new(&s))
                                    .collect::<Result<Vec<_>, _>>()?;
                            }
                            _ => {}
                        }
                    }

                    let kind = kind.ok_or_else(|| ManifestError::MissingField {
                        field: "kind".to_string(),
                    })?;

                    targets.push(TargetManifest {
                        name: target_name,
                        kind,
                        entry,
                        public_modules,
                    });
                }
                "dependency" => {
                    let alias = DependencyAlias::new(&block_name)?;
                    let mut package = None;
                    let mut path = None;
                    let mut version_req = None;

                    for (key, value) in &entries {
                        match key.as_str() {
                            "package" => {
                                let s = ManifestParser::parse_string_value(value)?;
                                package = Some(PackageNameRef::new(&s)?);
                            }
                            "path" => {
                                let s = ManifestParser::parse_string_value(value)?;
                                path = Some(PathRef::new(&s)?);
                            }
                            "version" => {
                                let s = ManifestParser::parse_string_value(value)?;
                                version_req = Some(VersionRequirement::new(&s)?);
                            }
                            _ => {}
                        }
                    }

                    let source = if path.is_some() {
                        DependencySource::Path
                    } else {
                        if package.is_none() {
                            return Err(ManifestError::RegistryDependencyMissingPackage {
                                alias: block_name,
                            });
                        }
                        DependencySource::Registry
                    };

                    dependencies.push(DependencyManifest {
                        alias,
                        package,
                        path,
                        version_req,
                        source,
                    });
                }
                "devDependency" => {
                    let alias = DependencyAlias::new(&block_name)?;
                    let mut package = None;
                    let mut path = None;
                    let mut version_req = None;

                    for (key, value) in &entries {
                        match key.as_str() {
                            "package" => {
                                let s = ManifestParser::parse_string_value(value)?;
                                package = Some(PackageNameRef::new(&s)?);
                            }
                            "path" => {
                                let s = ManifestParser::parse_string_value(value)?;
                                path = Some(PathRef::new(&s)?);
                            }
                            "version" => {
                                let s = ManifestParser::parse_string_value(value)?;
                                version_req = Some(VersionRequirement::new(&s)?);
                            }
                            _ => {}
                        }
                    }

                    let source = if path.is_some() {
                        DependencySource::Path
                    } else {
                        if package.is_none() {
                            return Err(ManifestError::RegistryDependencyMissingPackage {
                                alias: block_name,
                            });
                        }
                        DependencySource::Registry
                    };

                    dev_dependencies.push(DependencyManifest {
                        alias,
                        package,
                        path,
                        version_req,
                        source,
                    });
                }
                _ => {
                    return Err(ManifestError::ParseError);
                }
            }
        }

        let metadata = project_metadata.ok_or_else(|| ManifestError::MissingField {
            field: "project".to_string(),
        })?;

        Ok(ProjectManifest {
            project: metadata,
            targets,
            dependencies,
            dev_dependencies,
        })
    }

    pub fn validate(&self) -> Result<(), Vec<ManifestError>> {
        let mut errors = Vec::new();

        let mut target_names = std::collections::HashSet::new();
        for target in &self.targets {
            if !target_names.insert(target.name.as_str().to_string()) {
                errors.push(ManifestError::DuplicateTarget {
                    name: target.name.as_str().to_string(),
                });
            }
        }

        for dep in self.dependencies.iter().chain(self.dev_dependencies.iter()) {
            if dep.alias.as_str().is_empty() {
                errors.push(ManifestError::InvalidAlias {
                    alias: dep.alias.as_str().to_string(),
                });
            }

            match dep.source {
                DependencySource::Path => {
                    if dep.path.is_none() {
                        errors.push(ManifestError::PathDependencyMissingPath);
                    }
                }
                DependencySource::Registry => {
                    if dep.package.is_none() {
                        errors.push(ManifestError::RegistryDependencyMissingPackage {
                            alias: dep.alias.as_str().to_string(),
                        });
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_name_valid() {
        assert!(PackageName::new("hello").is_ok());
        assert!(PackageName::new("hello.world").is_ok());
        assert!(PackageName::new("hello-world").is_ok());
        assert!(PackageName::new("a.b.c.d.e").is_ok());
        assert!(PackageName::new("my-lib-v2").is_ok());
    }

    #[test]
    fn test_package_name_invalid() {
        assert!(PackageName::new("Hello").is_err());
        assert!(PackageName::new("foo.Bar").is_err());
        assert!(PackageName::new("hello world").is_err());
        assert!(PackageName::new(".hello").is_err());
        assert!(PackageName::new("hello.").is_err());
        assert!(PackageName::new("hello..world").is_err());
        assert!(PackageName::new("hello--world").is_err());
        assert!(PackageName::new("").is_err());
    }

    #[test]
    fn test_version_basic() {
        let v = SemanticVersion::new("1.0.0").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
        assert_eq!(v.to_string(), "1.0.0");
    }

    #[test]
    fn test_version_with_prerelease() {
        let v = SemanticVersion::new("1.2.3-alpha.1").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.prerelease.as_deref(), Some("alpha.1"));
        assert_eq!(v.to_string(), "1.2.3-alpha.1");
    }

    #[test]
    fn test_version_with_build_metadata() {
        let v = SemanticVersion::new("1.0.0+build.123").unwrap();
        assert_eq!(v.to_string(), "1.0.0+build.123");
    }

    #[test]
    fn test_version_with_both() {
        let v = SemanticVersion::new("1.0.0-beta+sha.abc123").unwrap();
        assert_eq!(v.to_string(), "1.0.0-beta+sha.abc123");
    }

    #[test]
    fn test_version_invalid() {
        assert!(SemanticVersion::new("1.0").is_err());
        assert!(SemanticVersion::new("1").is_err());
        assert!(SemanticVersion::new("a.b.c").is_err());
        assert!(SemanticVersion::new("").is_err());
    }

    #[test]
    fn test_version_comparison() {
        let v1 = SemanticVersion::new("1.0.0").unwrap();
        let v2 = SemanticVersion::new("1.0.1").unwrap();
        let v3 = SemanticVersion::new("1.1.0").unwrap();
        let v4 = SemanticVersion::new("2.0.0").unwrap();

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v3 < v4);
        assert!(v1 < v4);
    }

    #[test]
    fn test_version_prerelease_ordering() {
        let stable = SemanticVersion::new("1.0.0").unwrap();
        let pre = SemanticVersion::new("1.0.0-alpha").unwrap();
        assert!(pre < stable);
    }

    #[test]
    fn test_version_req_exact() {
        let req = VersionRequirement::new("1.5.0").unwrap();
        assert!(req.matches(&SemanticVersion::new("1.5.0").unwrap()));
        assert!(!req.matches(&SemanticVersion::new("1.5.1").unwrap()));
        assert!(!req.matches(&SemanticVersion::new("1.4.9").unwrap()));
    }

    #[test]
    fn test_version_req_caret() {
        let req = VersionRequirement::new("^1.2.3").unwrap();
        assert!(req.matches(&SemanticVersion::new("1.2.3").unwrap()));
        assert!(req.matches(&SemanticVersion::new("1.2.4").unwrap()));
        assert!(req.matches(&SemanticVersion::new("1.9.9").unwrap()));
        assert!(!req.matches(&SemanticVersion::new("2.0.0").unwrap()));
        assert!(!req.matches(&SemanticVersion::new("1.2.2").unwrap()));
    }

    #[test]
    fn test_version_req_caret_zero_major() {
        let req = VersionRequirement::new("^0.1.0").unwrap();
        assert!(req.matches(&SemanticVersion::new("0.1.0").unwrap()));
        assert!(req.matches(&SemanticVersion::new("0.1.5").unwrap()));
        assert!(!req.matches(&SemanticVersion::new("0.2.0").unwrap()));
        assert!(!req.matches(&SemanticVersion::new("1.0.0").unwrap()));
    }

    #[test]
    fn test_version_req_caret_zero_minor() {
        let req = VersionRequirement::new("^0.0.3").unwrap();
        assert!(req.matches(&SemanticVersion::new("0.0.3").unwrap()));
        assert!(!req.matches(&SemanticVersion::new("0.0.4").unwrap()));
        assert!(!req.matches(&SemanticVersion::new("0.1.0").unwrap()));
    }

    #[test]
    fn test_version_req_tilde() {
        let req = VersionRequirement::new("~1.2.3").unwrap();
        assert!(req.matches(&SemanticVersion::new("1.2.3").unwrap()));
        assert!(req.matches(&SemanticVersion::new("1.2.9").unwrap()));
        assert!(!req.matches(&SemanticVersion::new("1.3.0").unwrap()));
        assert!(!req.matches(&SemanticVersion::new("1.2.2").unwrap()));
    }

    #[test]
    fn test_version_req_wildcard() {
        let req = VersionRequirement::new("*").unwrap();
        assert!(req.matches(&SemanticVersion::new("0.0.0").unwrap()));
        assert!(req.matches(&SemanticVersion::new("999.999.999").unwrap()));
    }

    #[test]
    fn test_version_req_range_operators() {
        let gte = VersionRequirement::new(">=1.0.0").unwrap();
        assert!(gte.matches(&SemanticVersion::new("1.0.0").unwrap()));
        assert!(gte.matches(&SemanticVersion::new("2.0.0").unwrap()));
        assert!(!gte.matches(&SemanticVersion::new("0.9.9").unwrap()));

        let lt = VersionRequirement::new("<2.0.0").unwrap();
        assert!(lt.matches(&SemanticVersion::new("1.9.9").unwrap()));
        assert!(!lt.matches(&SemanticVersion::new("2.0.0").unwrap()));

        let gt = VersionRequirement::new(">1.0.0").unwrap();
        assert!(gt.matches(&SemanticVersion::new("1.0.1").unwrap()));
        assert!(!gt.matches(&SemanticVersion::new("1.0.0").unwrap()));

        let lte = VersionRequirement::new("<=2.0.0").unwrap();
        assert!(lte.matches(&SemanticVersion::new("2.0.0").unwrap()));
        assert!(lte.matches(&SemanticVersion::new("1.0.0").unwrap()));
        assert!(!lte.matches(&SemanticVersion::new("2.0.1").unwrap()));
    }

    #[test]
    fn test_version_req_partial() {
        let req = VersionRequirement::new("^1.2").unwrap();
        assert!(req.matches(&SemanticVersion::new("1.2.0").unwrap()));
        assert!(req.matches(&SemanticVersion::new("1.9.9").unwrap()));
        assert!(!req.matches(&SemanticVersion::new("2.0.0").unwrap()));
    }

    #[test]
    fn test_target_kind() {
        assert_eq!(
            TargetKind::parse("executable").unwrap(),
            TargetKind::Executable
        );
        assert_eq!(TargetKind::parse("library").unwrap(), TargetKind::Library);
        assert!(TargetKind::parse("test").is_err());
    }

    #[test]
    fn test_parse_valid_manifest() {
        let input = r#"
project {
    name = "hello"
    version = "1.0.0"
    language = "1.0"
}

target app {
    kind = "executable"
    entry = "main"
}

target lib {
    kind = "library"
    publicModules = ["api", "models"]
}

dependency json {
    package = "nexa.json"
    version = "^1.2"
}

devDependency test_utils {
    path = "../test-utils"
}
"#;
        let manifest = ProjectManifest::parse(input).unwrap();
        assert_eq!(manifest.project.name.as_str(), "hello");
        assert_eq!(manifest.project.version.to_string(), "1.0.0");
        assert_eq!(manifest.project.language.as_str(), "1.0");
        assert_eq!(manifest.targets.len(), 2);
        assert_eq!(manifest.targets[0].name.as_str(), "app");
        assert_eq!(manifest.targets[0].kind, TargetKind::Executable);
        assert_eq!(manifest.targets[1].name.as_str(), "lib");
        assert_eq!(manifest.targets[1].kind, TargetKind::Library);
        assert_eq!(manifest.targets[1].public_modules.len(), 2);
        assert_eq!(manifest.targets[1].public_modules[0].as_str(), "api");
        assert_eq!(manifest.targets[1].public_modules[1].as_str(), "models");
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(manifest.dependencies[0].alias.as_str(), "json");
        assert_eq!(
            manifest.dependencies[0].package.as_ref().unwrap().as_str(),
            "nexa.json"
        );
        assert_eq!(manifest.dependencies[0].source, DependencySource::Registry);
        assert_eq!(manifest.dev_dependencies.len(), 1);
        assert_eq!(manifest.dev_dependencies[0].alias.as_str(), "test_utils");
        assert_eq!(manifest.dev_dependencies[0].source, DependencySource::Path);
        assert_eq!(
            manifest.dev_dependencies[0].path.as_ref().unwrap().as_str(),
            "../test-utils"
        );
    }

    #[test]
    fn test_parse_with_comments() {
        let input = r#"
// This is a comment
project {
    name = "my-app" // inline comment
    version = "0.1.0"
    language = "1.0"
}
// another comment
target main {
    kind = "executable"
    entry = "main"
}
"#;
        let manifest = ProjectManifest::parse(input).unwrap();
        assert_eq!(manifest.project.name.as_str(), "my-app");
    }

    #[test]
    fn test_parse_missing_project_block() {
        let input = r#"
target app {
    kind = "executable"
}
"#;
        assert!(ProjectManifest::parse(input).is_err());
    }

    #[test]
    fn test_parse_missing_required_field() {
        let input = r#"
project {
    version = "1.0.0"
    language = "1.0"
}
"#;
        let err = ProjectManifest::parse(input).unwrap_err();
        assert!(matches!(err, ManifestError::MissingField { .. }));
    }

    #[test]
    fn test_validate_duplicate_targets() {
        let input = r#"
project {
    name = "test"
    version = "1.0.0"
    language = "1.0"
}

target app {
    kind = "executable"
    entry = "main"
}

target app {
    kind = "library"
}
"#;
        let manifest = ProjectManifest::parse(input).unwrap();
        let errors = manifest.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ManifestError::DuplicateTarget { .. })));
    }

    #[test]
    fn test_validate_invalid_alias() {
        let input = r#"
project {
    name = "test"
    version = "1.0.0"
    language = "1.0"
}

dependency "" {
    package = "nexa.json"
    version = "^1.0"
}
"#;
        let manifest = ProjectManifest::parse(input).unwrap();
        let errors = manifest.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ManifestError::InvalidAlias { .. })));
    }

    #[test]
    fn test_validate_registry_dependency_missing_package() {
        let input = r#"
project {
    name = "test"
    version = "1.0.0"
    language = "1.0"
}

dependency mydep {
    version = "^1.0"
}
"#;
        let err = ProjectManifest::parse(input).unwrap_err();
        assert!(matches!(
            err,
            ManifestError::RegistryDependencyMissingPackage { .. }
        ));
    }

    #[test]
    fn test_validate_path_dependency_missing_path() {
        let input = r#"
project {
    name = "test"
    version = "1.0.0"
    language = "1.0"
}

dependency mydep {
    path = "../mydep"
    version = "^1.0"
}
"#;
        let manifest = ProjectManifest::parse(input).unwrap();
        assert_eq!(manifest.dependencies[0].source, DependencySource::Path);
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_empty_public_modules() {
        let input = r#"
project {
    name = "test"
    version = "1.0.0"
    language = "1.0"
}

target lib {
    kind = "library"
    publicModules = []
}
"#;
        let manifest = ProjectManifest::parse(input).unwrap();
        assert!(manifest.targets[0].public_modules.is_empty());
    }

    #[test]
    fn test_manifest_validate_ok() {
        let input = r#"
project {
    name = "valid-project"
    version = "1.0.0"
    language = "1.0"
}

target app {
    kind = "executable"
    entry = "main"
}

dependency dep1 {
    package = "nexa.dep1"
    version = "^1.0"
}

devDependency dep2 {
    path = "./dep2"
}
"#;
        let manifest = ProjectManifest::parse(input).unwrap();
        assert!(manifest.validate().is_ok());
    }
}
