//! Project type detection from marker files.
//!
//! Identifies project types by scanning for marker files like `Cargo.toml`,
//! `package.json`, etc. Each marker has an associated confidence score
//! indicating how definitively it identifies the project type.

use std::path::{Path, PathBuf};

/// Known project types that can be detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Java,
    CSharp,
    Ruby,
    Elixir,
    Php,
    Swift,
    Kotlin,
    Scala,
    Haskell,
    Clojure,
    Cpp,
    C,
    Zig,
    Nim,
    Unknown,
}

impl ProjectType {
    /// Get a human-readable name for the project type.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Node => "Node.js",
            Self::Python => "Python",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::CSharp => "C#",
            Self::Ruby => "Ruby",
            Self::Elixir => "Elixir",
            Self::Php => "PHP",
            Self::Swift => "Swift",
            Self::Kotlin => "Kotlin",
            Self::Scala => "Scala",
            Self::Haskell => "Haskell",
            Self::Clojure => "Clojure",
            Self::Cpp => "C++",
            Self::C => "C",
            Self::Zig => "Zig",
            Self::Nim => "Nim",
            Self::Unknown => "Unknown",
        }
    }

    /// Get a lowercase identifier for the project type.
    pub fn id(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Node => "node",
            Self::Python => "python",
            Self::Go => "go",
            Self::Java => "java",
            Self::CSharp => "csharp",
            Self::Ruby => "ruby",
            Self::Elixir => "elixir",
            Self::Php => "php",
            Self::Swift => "swift",
            Self::Kotlin => "kotlin",
            Self::Scala => "scala",
            Self::Haskell => "haskell",
            Self::Clojure => "clojure",
            Self::Cpp => "cpp",
            Self::C => "c",
            Self::Zig => "zig",
            Self::Nim => "nim",
            Self::Unknown => "unknown",
        }
    }
}

/// A marker file that indicates a project type.
#[derive(Debug, Clone)]
pub struct ProjectMarker {
    /// File name or glob pattern to match.
    pub pattern: &'static str,
    /// Project type this marker indicates.
    pub project_type: ProjectType,
    /// Confidence score (0.0-1.0) for how definitively this identifies the project.
    pub confidence: f32,
    /// Whether this is a glob pattern (vs exact filename).
    pub is_glob: bool,
}

impl ProjectMarker {
    /// Create a new marker with exact filename match.
    pub const fn new(pattern: &'static str, project_type: ProjectType, confidence: f32) -> Self {
        Self {
            pattern,
            project_type,
            confidence,
            is_glob: false,
        }
    }

    /// Create a new marker with glob pattern.
    pub const fn glob(pattern: &'static str, project_type: ProjectType, confidence: f32) -> Self {
        Self {
            pattern,
            project_type,
            confidence,
            is_glob: true,
        }
    }

    /// Check if this marker matches a given filename.
    pub fn matches(&self, filename: &str) -> bool {
        if self.is_glob {
            self.glob_matches(filename)
        } else {
            self.pattern == filename
        }
    }

    fn glob_matches(&self, filename: &str) -> bool {
        // Simple glob matching for common patterns like "*.csproj"
        if let Some(suffix) = self.pattern.strip_prefix('*') {
            filename.ends_with(suffix)
        } else if let Some(prefix) = self.pattern.strip_suffix('*') {
            filename.starts_with(prefix)
        } else {
            self.pattern == filename
        }
    }
}

/// Result of detecting a project type.
#[derive(Debug, Clone)]
pub struct DetectedProject {
    /// The detected project type.
    pub project_type: ProjectType,
    /// Confidence score (0.0-1.0).
    pub confidence: f32,
    /// Path to the marker file that triggered detection.
    pub marker_path: PathBuf,
    /// The marker pattern that matched.
    pub marker_pattern: String,
}

/// Trait for project type detection.
pub trait ProjectDetector: Send + Sync {
    /// Detect all project types in the given directory.
    fn detect(&self, path: &Path) -> Vec<DetectedProject>;

    /// Check if a specific marker file exists.
    fn has_marker(&self, path: &Path, pattern: &str) -> bool;

    /// Get all detected project types with confidence scores, sorted by confidence.
    fn detect_with_confidence(&self, path: &Path) -> Vec<(ProjectType, f32)> {
        let mut results = self.detect(path);
        // Sort by confidence descending
        results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // Dedupe by project type, keeping highest confidence
        let mut seen = std::collections::HashSet::new();
        results
            .into_iter()
            .filter_map(|d| {
                if seen.insert(d.project_type) {
                    Some((d.project_type, d.confidence))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get the primary (highest confidence) project type.
    fn primary_type(&self, path: &Path) -> Option<(ProjectType, f32)> {
        self.detect_with_confidence(path).into_iter().next()
    }
}

/// Default project detector with built-in marker registry.
#[derive(Debug, Clone)]
pub struct DefaultDetector {
    markers: Vec<ProjectMarker>,
}

impl Default for DefaultDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultDetector {
    /// Create a new detector with the default marker registry.
    pub fn new() -> Self {
        Self {
            markers: default_markers(),
        }
    }

    /// Create a detector with custom markers.
    pub fn with_markers(markers: Vec<ProjectMarker>) -> Self {
        Self { markers }
    }

    /// Add a custom marker to the registry.
    pub fn add_marker(&mut self, marker: ProjectMarker) {
        self.markers.push(marker);
    }

    /// Get the marker registry.
    pub fn markers(&self) -> &[ProjectMarker] {
        &self.markers
    }
}

impl ProjectDetector for DefaultDetector {
    fn detect(&self, path: &Path) -> Vec<DetectedProject> {
        let mut results = Vec::new();

        // Read directory entries
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(_) => return results,
        };

        // Collect filenames
        let filenames: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();

        // Check each marker against filenames
        for marker in &self.markers {
            for filename in &filenames {
                if marker.matches(filename) {
                    results.push(DetectedProject {
                        project_type: marker.project_type,
                        confidence: marker.confidence,
                        marker_path: path.join(filename),
                        marker_pattern: marker.pattern.to_string(),
                    });
                }
            }
        }

        results
    }

    fn has_marker(&self, path: &Path, pattern: &str) -> bool {
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(_) => return false,
        };

        for entry in entries.filter_map(|e| e.ok()) {
            if let Ok(name) = entry.file_name().into_string() {
                // Check for exact match or glob pattern
                if name == pattern {
                    return true;
                }
                // Simple glob matching
                if let Some(suffix) = pattern.strip_prefix('*') {
                    if name.ends_with(suffix) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Default marker registry covering common project types.
fn default_markers() -> Vec<ProjectMarker> {
    vec![
        // Rust - definitive
        ProjectMarker::new("Cargo.toml", ProjectType::Rust, 1.0),
        // Node.js / JavaScript
        ProjectMarker::new("package.json", ProjectType::Node, 0.9),
        ProjectMarker::new("package-lock.json", ProjectType::Node, 0.8),
        ProjectMarker::new("yarn.lock", ProjectType::Node, 0.8),
        ProjectMarker::new("pnpm-lock.yaml", ProjectType::Node, 0.8),
        ProjectMarker::new("bun.lockb", ProjectType::Node, 0.8),
        // Python
        ProjectMarker::new("pyproject.toml", ProjectType::Python, 1.0),
        ProjectMarker::new("setup.py", ProjectType::Python, 0.9),
        ProjectMarker::new("requirements.txt", ProjectType::Python, 0.8),
        ProjectMarker::new("Pipfile", ProjectType::Python, 0.9),
        ProjectMarker::new("poetry.lock", ProjectType::Python, 0.9),
        ProjectMarker::new("uv.lock", ProjectType::Python, 0.9),
        // Go - definitive
        ProjectMarker::new("go.mod", ProjectType::Go, 1.0),
        ProjectMarker::new("go.sum", ProjectType::Go, 0.8),
        // Java
        ProjectMarker::new("pom.xml", ProjectType::Java, 1.0),
        ProjectMarker::new("build.gradle", ProjectType::Java, 0.9),
        ProjectMarker::new("build.gradle.kts", ProjectType::Kotlin, 0.9),
        ProjectMarker::new("settings.gradle", ProjectType::Java, 0.8),
        // C# / .NET
        ProjectMarker::glob("*.csproj", ProjectType::CSharp, 1.0),
        ProjectMarker::glob("*.sln", ProjectType::CSharp, 0.9),
        ProjectMarker::new("Directory.Build.props", ProjectType::CSharp, 0.7),
        // Ruby
        ProjectMarker::new("Gemfile", ProjectType::Ruby, 1.0),
        ProjectMarker::new("Gemfile.lock", ProjectType::Ruby, 0.8),
        ProjectMarker::glob("*.gemspec", ProjectType::Ruby, 0.9),
        // Elixir
        ProjectMarker::new("mix.exs", ProjectType::Elixir, 1.0),
        ProjectMarker::new("mix.lock", ProjectType::Elixir, 0.8),
        // PHP
        ProjectMarker::new("composer.json", ProjectType::Php, 1.0),
        ProjectMarker::new("composer.lock", ProjectType::Php, 0.8),
        // Swift
        ProjectMarker::new("Package.swift", ProjectType::Swift, 1.0),
        ProjectMarker::glob("*.xcodeproj", ProjectType::Swift, 0.8),
        ProjectMarker::glob("*.xcworkspace", ProjectType::Swift, 0.8),
        // Kotlin
        ProjectMarker::new("build.gradle.kts", ProjectType::Kotlin, 0.9),
        // Scala
        ProjectMarker::new("build.sbt", ProjectType::Scala, 1.0),
        // Haskell
        ProjectMarker::glob("*.cabal", ProjectType::Haskell, 1.0),
        ProjectMarker::new("stack.yaml", ProjectType::Haskell, 0.9),
        ProjectMarker::new("cabal.project", ProjectType::Haskell, 0.9),
        // Clojure
        ProjectMarker::new("project.clj", ProjectType::Clojure, 1.0),
        ProjectMarker::new("deps.edn", ProjectType::Clojure, 1.0),
        // C/C++
        ProjectMarker::new("CMakeLists.txt", ProjectType::Cpp, 0.9),
        ProjectMarker::new("Makefile", ProjectType::C, 0.6),
        ProjectMarker::new("configure.ac", ProjectType::C, 0.7),
        ProjectMarker::new("meson.build", ProjectType::Cpp, 0.8),
        ProjectMarker::new("conanfile.txt", ProjectType::Cpp, 0.8),
        ProjectMarker::new("vcpkg.json", ProjectType::Cpp, 0.8),
        // Zig
        ProjectMarker::new("build.zig", ProjectType::Zig, 1.0),
        ProjectMarker::new("build.zig.zon", ProjectType::Zig, 1.0),
        // Nim
        ProjectMarker::glob("*.nimble", ProjectType::Nim, 1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::TempDir;

    fn setup_project(files: &[&str]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for file in files {
            File::create(dir.path().join(file)).unwrap();
        }
        dir
    }

    #[test]
    fn detect_rust_project() {
        let dir = setup_project(&["Cargo.toml", "src"]);
        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        assert!(!results.is_empty());
        let rust = results.iter().find(|r| r.project_type == ProjectType::Rust);
        assert!(rust.is_some());
        assert!((rust.unwrap().confidence - 1.0).abs() < 0.001);
    }

    #[test]
    fn detect_node_project() {
        let dir = setup_project(&["package.json", "node_modules"]);
        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        assert!(!results.is_empty());
        let node = results.iter().find(|r| r.project_type == ProjectType::Node);
        assert!(node.is_some());
        assert!(node.unwrap().confidence >= 0.9);
    }

    #[test]
    fn detect_python_project() {
        let dir = setup_project(&["pyproject.toml", "src"]);
        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        let python = results
            .iter()
            .find(|r| r.project_type == ProjectType::Python);
        assert!(python.is_some());
        assert!((python.unwrap().confidence - 1.0).abs() < 0.001);
    }

    #[test]
    fn detect_go_project() {
        let dir = setup_project(&["go.mod", "go.sum", "main.go"]);
        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        let go = results.iter().find(|r| r.project_type == ProjectType::Go);
        assert!(go.is_some());
        assert!((go.unwrap().confidence - 1.0).abs() < 0.001);
    }

    #[test]
    fn detect_multi_language_project() {
        // A project with both Rust and Python
        let dir = setup_project(&["Cargo.toml", "pyproject.toml"]);
        let detector = DefaultDetector::new();
        let results = detector.detect_with_confidence(dir.path());

        assert!(results.len() >= 2);
        // Both should have high confidence
        let rust = results.iter().find(|(t, _)| *t == ProjectType::Rust);
        let python = results.iter().find(|(t, _)| *t == ProjectType::Python);
        assert!(rust.is_some());
        assert!(python.is_some());
    }

    #[test]
    fn detect_empty_directory() {
        let dir = TempDir::new().unwrap();
        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        assert!(results.is_empty());
    }

    #[test]
    fn primary_type_returns_highest_confidence() {
        let dir = setup_project(&["package.json", "requirements.txt"]);
        let detector = DefaultDetector::new();
        let primary = detector.primary_type(dir.path());

        assert!(primary.is_some());
        let (project_type, confidence) = primary.unwrap();
        // Node has 0.9 confidence, Python requirements.txt has 0.8
        assert_eq!(project_type, ProjectType::Node);
        assert!(confidence >= 0.9);
    }

    #[test]
    fn has_marker_exact_match() {
        let dir = setup_project(&["Cargo.toml"]);
        let detector = DefaultDetector::new();

        assert!(detector.has_marker(dir.path(), "Cargo.toml"));
        assert!(!detector.has_marker(dir.path(), "package.json"));
    }

    #[test]
    fn has_marker_glob_match() {
        let dir = setup_project(&["MyProject.csproj"]);
        let detector = DefaultDetector::new();

        assert!(detector.has_marker(dir.path(), "*.csproj"));
        assert!(!detector.has_marker(dir.path(), "*.sln"));
    }

    #[test]
    fn glob_marker_detection() {
        let dir = setup_project(&["MyApp.csproj", "Program.cs"]);
        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        let csharp = results
            .iter()
            .find(|r| r.project_type == ProjectType::CSharp);
        assert!(csharp.is_some());
        assert!((csharp.unwrap().confidence - 1.0).abs() < 0.001);
    }

    #[test]
    fn project_type_name_and_id() {
        assert_eq!(ProjectType::Rust.name(), "Rust");
        assert_eq!(ProjectType::Rust.id(), "rust");
        assert_eq!(ProjectType::Node.name(), "Node.js");
        assert_eq!(ProjectType::Node.id(), "node");
        assert_eq!(ProjectType::CSharp.name(), "C#");
        assert_eq!(ProjectType::CSharp.id(), "csharp");
    }

    #[test]
    fn custom_marker() {
        let dir = setup_project(&["custom.marker"]);
        let mut detector = DefaultDetector::new();
        detector.add_marker(ProjectMarker::new(
            "custom.marker",
            ProjectType::Unknown,
            0.5,
        ));

        let results = detector.detect(dir.path());
        let custom = results
            .iter()
            .find(|r| r.marker_pattern == "custom.marker");
        assert!(custom.is_some());
    }
}
