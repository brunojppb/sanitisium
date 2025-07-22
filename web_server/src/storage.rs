use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

/// A file storage service that provides functionality to store and retrieve files
pub struct FileStorage {
    /// Base directory where files will be stored
    base_dir: String,
}

impl FileStorage {
    /// Creates a new FileStorage instance with the specified base directory
    ///
    /// # Arguments
    /// * `base_dir` - The base directory where files will be stored
    ///
    /// # Returns
    /// A new FileStorage instance
    pub fn new(base_dir: String) -> Self {
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
    /// use std::path::Path;
    ///
    /// let storage = FileStorage::new("./storage".to_string());
    /// let data = b"Hello, World!";
    /// storage.store_file(&Path::new("test.txt"), data).unwrap();
    /// ```
    pub fn store_file<P>(&self, path: &P, data: &[u8]) -> Result<(), io::Error>
    where
        P: AsRef<Path>,
    {
        let full_path = Path::new(&self.base_dir).join(path.as_ref());

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
    /// use std::path::Path;
    ///
    /// let storage = FileStorage::new("./storage".to_string());
    /// if let Some(file) = storage.get_file(&Path::new("test.txt")) {
    ///     // File exists, you can read from it
    /// }
    /// ```
    pub fn get_file<P>(&self, path: &P) -> Option<File>
    where
        P: AsRef<Path>,
    {
        let full_path = Path::new(&self.base_dir).join(path.as_ref());

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
    pub fn file_exists<P>(&self, path: &P) -> bool
    where
        P: AsRef<Path>,
    {
        let full_path = Path::new(&self.base_dir).join(path.as_ref());
        full_path.exists() && full_path.is_file()
    }

    /// Deletes a file at the given path
    ///
    /// # Arguments
    /// * `path` - The path of the file to delete (relative to base_dir)
    ///
    /// # Returns
    /// `Ok(())` if the file was deleted successfully, otherwise an `io::Error`
    pub fn delete_file<P>(&self, path: &P) -> Result<(), io::Error>
    where
        P: AsRef<Path>,
    {
        let full_path = Path::new(&self.base_dir).join(path.as_ref());
        fs::remove_file(full_path)
    }

    /// Gets the base directory of this storage instance
    pub fn base_dir(&self) -> &str {
        &self.base_dir
    }
}

impl Default for FileStorage {
    /// Creates a default FileStorage instance with "./storage" as the base directory
    fn default() -> Self {
        Self::new("./storage".to_string())
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
        storage.store_file(&test_path, test_data).unwrap();

        // Retrieve the file
        let mut file = storage.get_file(&test_path).unwrap();
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).unwrap();

        assert_eq!(contents, test_data);
    }

    #[test]
    fn test_get_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path().to_string_lossy().to_string());

        let result = storage.get_file(&"nonexistent.txt");
        assert!(result.is_none());
    }

    #[test]
    fn test_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path().to_string_lossy().to_string());

        let test_data = b"Test data";
        let test_path = "exists_test.txt";

        assert!(!storage.file_exists(&test_path));

        storage.store_file(&test_path, test_data).unwrap();
        assert!(storage.file_exists(&test_path));
    }

    #[test]
    fn test_store_file_with_nested_path() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::new(temp_dir.path().to_string_lossy().to_string());

        let test_data = b"Nested file content";
        let test_path = "nested/directory/test.txt";

        storage.store_file(&test_path, test_data).unwrap();

        let mut file = storage.get_file(&test_path).unwrap();
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

        storage.store_file(&test_path, test_data).unwrap();
        assert!(storage.file_exists(&test_path));

        storage.delete_file(&test_path).unwrap();
        assert!(!storage.file_exists(&test_path));
    }
}
