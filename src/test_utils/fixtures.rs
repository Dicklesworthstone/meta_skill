use std::path::PathBuf;

use tempfile::TempDir;

/// Test fixture providing isolated filesystem environment.
pub struct UnitTestFixture {
    pub temp_dir: TempDir,
    pub data_path: PathBuf,
}

impl UnitTestFixture {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let data_path = temp_dir.path().to_path_buf();

        println!("[FIXTURE] Created temp directory: {:?}", data_path);

        Self {
            temp_dir,
            data_path,
        }
    }

    /// Create a test file with content.
    pub fn create_file(&self, relative_path: &str, content: &str) -> PathBuf {
        let full_path = self.data_path.join(relative_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create parent dirs");
        }
        std::fs::write(&full_path, content).expect("Failed to write file");
        println!(
            "[FIXTURE] Created file: {:?} ({} bytes)",
            full_path,
            content.len()
        );
        full_path
    }

    /// Create a test skill file.
    pub fn create_skill(&self, name: &str, content: &str) -> PathBuf {
        self.create_file(&format!("skills/{}/SKILL.md", name), content)
    }
}

impl Drop for UnitTestFixture {
    fn drop(&mut self) {
        println!("[FIXTURE] Cleaning up temp directory: {:?}", self.data_path);
    }
}
