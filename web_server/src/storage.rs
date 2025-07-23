use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

/// A file storage service that provides functionality to store and retrieve files
#[derive(Debug)]
pub struct FileStorage<P: AsRef<Path>> {
    /// Base directory where files will be stored
    base_dir: P,
}

impl<P: AsRef<Path>> FileStorage<P> {
    /// Creates a new FileStorage instance with the specified base directory
    ///
    /// # Arguments
    /// * `base_dir` - The base directory where files will be stored
    ///
    /// # Returns
    /// A new FileStorage instance
    pub fn new(base_dir: P) -> Self {
        Self { base_dir }
    }

    /// Stores a file from a byte slice at the given path
    ///
    /// # Arguments
    /// * `path` - The path where the file should be stored (relative to base_dir)
    /// * `data` - The file content as a byte slice
    ///
    /// # Returns
    /// `Ok(())` if the file was stored successfully, otherwise an `io::Error`
    ///
    /// # Examples
    /// ```
    /// use web_server::storage::FileStorage;
    ///
    /// let storage = FileStorage::new("./storage".to_string());
    /// let data = b"Hello, World!";
    /// storage.store_file(&"test.txt".into(), data).unwrap();
    /// ```
    pub fn store_file(&self, path: &P, data: &[u8]) -> Result<(), io::Error> {
        let full_path = &self.base_dir.as_ref().join(path.as_ref());

        // Create parent directories if they don't exist
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create and write to the file
        let mut file = File::create(full_path)?;
        file.write_all(data)?;
        file.sync_all()?; // Ensure data is written to disk

        Ok(())
    }

    /// Retrieves a file from the given path
    ///
    /// # Arguments
    /// * `path` - The path of the file to retrieve (relative to base_dir)
    ///
    /// # Returns
    /// `Some(File)` if the file exists, `None` otherwise
    ///
    /// # Examples
    /// ```
    /// use web_server::storage::FileStorage;
    ///
    /// let storage = FileStorage::new("./storage".to_string());
    /// if let Some(file) = storage.get_file(&"test.txt".into()) {
    ///     // File exists, you can read from it
    /// }
    /// ```
    pub fn get_file(&self, path: &P) -> Option<File> {
        let full_path = &self.base_dir.as_ref().join(path.as_ref());

        // Check if file exists and try to open it
        if full_path.exists() && full_path.is_file() {
            File::open(full_path).ok()
        } else {
            None
        }
    }

    /// Checks if a file exists at the given path
    ///
    /// # Arguments
    /// * `path` - The path to check (relative to base_dir)
    ///
    /// # Returns
    /// `true` if the file exists, `false` otherwise
    pub fn file_exists(&self, path: &P) -> bool {
        let full_path = &self.base_dir.as_ref().join(path.as_ref());
        full_path.exists() && full_path.is_file()
    }

    /// Deletes a file at the given path
    ///
    /// # Arguments
    /// * `path` - The path of the file to delete (relative to base_dir)
    ///
    /// # Returns
    /// `Ok(())` if the file was deleted successfully, otherwise an `io::Error`
    pub fn delete_file(&self, path: &P) -> Result<(), io::Error> {
        let full_path = &self.base_dir.as_ref().join(path.as_ref());
        fs::remove_file(full_path)
    }

    /// Gets the base directory of this storage instance
    pub fn base_dir(&self) -> &str {
        self.base_dir
            .as_ref()
            .to_str()
            .expect("base_dir should be a valid string")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::TempDir;

    #[test]
    fn test_store_and_get_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path().to_string_lossy().to_string());

        let test_data = b"Hello, World!";
        let test_path = "test.txt";

        // Store the file
        storage.store_file(&test_path.into(), test_data).unwrap();

        // Retrieve the file
        let mut file = storage.get_file(&test_path.into()).unwrap();
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).unwrap();

        assert_eq!(contents, test_data);
    }

    #[test]
    fn test_get_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path().to_string_lossy().to_string());

        let result = storage.get_file(&"nonexistent.txt".into());
        assert!(result.is_none());
    }

    #[test]
    fn test_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path().to_string_lossy().to_string());

        let test_data = b"Test data";
        let test_path = "exists_test.txt";

        assert!(!storage.file_exists(&test_path.into()));

        storage.store_file(&test_path.into(), test_data).unwrap();
        assert!(storage.file_exists(&test_path.into()));
    }

    #[test]
    fn test_store_file_with_nested_path() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path().to_string_lossy().to_string());

        let test_data = b"Nested file content";
        let test_path = "nested/directory/test.txt";

        storage.store_file(&test_path.into(), test_data).unwrap();

        let mut file = storage.get_file(&test_path.into()).unwrap();
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).unwrap();

        assert_eq!(contents, test_data);
    }

    #[test]
    fn test_delete_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path().to_string_lossy().to_string());

        let test_data = b"To be deleted";
        let test_path = "delete_me.txt";

        storage.store_file(&test_path.into(), test_data).unwrap();
        assert!(storage.file_exists(&test_path.into()));

        storage.delete_file(&test_path.into()).unwrap();
        assert!(!storage.file_exists(&test_path.into()));
    }
}
