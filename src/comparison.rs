//! File comparison functions for the tbcompare tool.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use log::info;
use anyhow::{Context, Result};

/// Represents the differences between two files
#[derive(Debug, Clone)]
pub struct FileDifferences {
    /// Lines that exist only in the first file
    pub only_in_first: Vec<String>,
    /// Lines that exist only in the second file
    pub only_in_second: Vec<String>,
}

/// Compares two files using system commands for efficiency
/// 
/// # Arguments
/// 
/// * `file1_path` - Path to the first file
/// * `file2_path` - Path to the second file
/// 
/// # Returns
/// 
/// A Result containing either the differences or an error
pub fn compare_files<P: AsRef<Path>>(file1_path: P, file2_path: P) -> Result<Option<FileDifferences>> {
    let file1_path = file1_path.as_ref();
    let file2_path = file2_path.as_ref();
    
    // Check if files exist
    if !file1_path.exists() {
        anyhow::bail!("File {} does not exist", file1_path.display());
    }
    
    if !file2_path.exists() {
        anyhow::bail!("File {} does not exist", file2_path.display());
    }
    
    // Try using system commands for comparison first (more efficient for large files)
    // On Windows, use fc.exe; on Unix-like systems, use diff
    #[cfg(windows)]
    {
        // Use fc.exe on Windows
        let output = Command::new("fc.exe")
            .arg("/L")  // Compare as text files
            .arg(file1_path)
            .arg(file2_path)
            .output()
            .context("Failed to execute fc.exe command")?;
            
        // fc.exe returns 0 if files are identical, 1 if different, 2 if error
        match output.status.code() {
            Some(0) => {
                // Files are identical
                info!("{} and {} are identical", file1_path.display(), file2_path.display());
                return Ok(None);
            }
            Some(1) => {
                // Files are different, fall through to detailed comparison
            }
            _ => {
                // Error occurred, fall through to detailed comparison
                info!("fc.exe failed, falling back to detailed comparison");
            }
        }
    }
    
    #[cfg(not(windows))]
    {
        // Use diff on Unix-like systems
        let output = Command::new("diff")
            .arg("-q")  // Quiet mode - just report if files differ
            .arg(file1_path)
            .arg(file2_path)
            .output()
            .context("Failed to execute diff command")?;
            
        if output.status.success() {
            // Files are identical (diff returned 0)
            info!("{} and {} are identical", file1_path.display(), file2_path.display());
            return Ok(None);
        } else if !output.stderr.is_empty() {
            // Error occurred, fall through to detailed comparison
            info!("diff failed, falling back to detailed comparison");
        }
        // If diff succeeded but files are different, we fall through to detailed comparison
    }
    
    // If system commands couldn't determine identity or we need detailed differences,
    // fall back to our detailed comparison implementation
    
    // Read and process files
    let lines1 = super::file_utils::read_and_process_file(file1_path)
        .with_context(|| format!("Failed to read and process file: {}", file1_path.display()))?;
    let lines2 = super::file_utils::read_and_process_file(file2_path)
        .with_context(|| format!("Failed to read and process file: {}", file2_path.display()))?;
    
    // Convert to sets for comparison
    let set1: HashSet<_> = lines1.into_iter().collect();
    let set2: HashSet<_> = lines2.into_iter().collect();
    
    // Find differences
    let only_in_first: Vec<_> = set1.difference(&set2).cloned().collect();
    let only_in_second: Vec<_> = set2.difference(&set1).cloned().collect();
    
    if only_in_first.is_empty() && only_in_second.is_empty() {
        info!("{} and {} have no differences", file1_path.display(), file2_path.display());
        Ok(None)
    } else {
        info!("{} and {} have differences", file1_path.display(), file2_path.display());
        if !only_in_first.is_empty() {
            info!("Lines only in {}:", file1_path.display());
            for line in &only_in_first {
                info!("  {}", line);
            }
        }
        if !only_in_second.is_empty() {
            info!("Lines only in {}:", file2_path.display());
            for line in &only_in_second {
                info!("  {}", line);
            }
        }
        Ok(Some(FileDifferences {
            only_in_first,
            only_in_second,
        }))
    }
}

/// Generates file name pairs based on the actual files in the directories
/// Files are matched based on the pattern SC_aaaaaaaa_yyyymmdd_tttN_AXX_Z where
/// aaaaaaaa, yyyymmdd, and AXX must be the same, but tttN (version) may differ.
///
/// # Arguments
///
/// * `dir1_path` - Path to the first directory
/// * `dir2_path` - Path to the second directory
///
/// # Returns
///
/// A vector of tuples containing file path pairs
pub fn generate_file_pairs<P: AsRef<Path>>(dir1_path: P, dir2_path: P) -> Result<Vec<(PathBuf, PathBuf)>> {
    let dir1_path = dir1_path.as_ref();
    let dir2_path = dir2_path.as_ref();
    
    // Read files from both directories
    let files1: Vec<_> = fs::read_dir(dir1_path)
        .with_context(|| format!("Failed to read directory: {}", dir1_path.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.path())
        .collect();
        
    let files2: Vec<_> = fs::read_dir(dir2_path)
        .with_context(|| format!("Failed to read directory: {}", dir2_path.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.path())
        .collect();
    
    let mut file_pairs = Vec::new();
    
    // Create a hash map for files in dir2 for O(1) lookup
    let mut dir2_map = std::collections::HashMap::new();
    
    // Populate the hash map with files from dir2
    for file2_path in &files2 {
        if let Some(file2_stem) = file2_path.file_stem().and_then(|n| n.to_str()) {
            let parts2: Vec<&str> = file2_stem.split('_').collect();
            
            // Check if the file name matches the expected pattern
            if parts2.len() >= 6 && parts2[0] == "SC" && parts2[parts2.len()-1] == "Z" {
                // Extract the parts that must match: aaaaaaaa, yyyymmdd, AXX
                let key2 = format!("{}_{}_{}", parts2[1], parts2[2], parts2[parts2.len()-2]);
                
                // Store the file path in the hash map
                dir2_map.insert(key2, file2_path.clone());
            }
        }
    }
    
    // For each file in dir1, find the corresponding file in dir2 using the hash map
    for file1_path in &files1 {
        if let Some(file1_stem) = file1_path.file_stem().and_then(|n| n.to_str()) {
            let parts1: Vec<&str> = file1_stem.split('_').collect();
            
            // Check if the file name matches the expected pattern
            if parts1.len() >= 6 && parts1[0] == "SC" && parts1[parts1.len()-1] == "Z" {
                // Extract the parts that must match: aaaaaaaa, yyyymmdd, AXX
                let key1 = format!("{}_{}_{}", parts1[1], parts1[2], parts1[parts1.len()-2]);
                
                // Look up the matching file in dir2 using the hash map
                if let Some(file2_path) = dir2_map.get(&key1) {
                    file_pairs.push((file1_path.clone(), file2_path.clone()));
                }
            }
        }
    }
    
    info!("生成了 {} 个文件对", file_pairs.len());
    Ok(file_pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::Builder;
    use anyhow::Result;

    #[test]
    fn test_compare_files_identical() -> Result<()> {
        let dir = Builder::new().prefix("tbcompare_test").tempdir()?;
        let file1_path = dir.path().join("file1.txt");
        let file2_path = dir.path().join("file2.txt");

        // Create test files with identical content (after skipping first line)
        let content = "Header line\nLine 2\nLine 1\nLine 3\n";
        fs::write(&file1_path, content)?;
        fs::write(&file2_path, content)?;

        let result = compare_files(&file1_path, &file2_path)?;
        assert!(result.is_none()); // No differences expected

        Ok(())
    }

    #[test]
    fn test_compare_files_different() -> Result<()> {
        let dir = Builder::new().prefix("tbcompare_test").tempdir()?;
        let file1_path = dir.path().join("file1.txt");
        let file2_path = dir.path().join("file2.txt");

        // Create test files with different content (after skipping first line)
        fs::write(&file1_path, "Header line\nLine 2\nLine 1\nLine 3\n")?;
        fs::write(&file2_path, "Header line\nLine 2\nLine 4\nLine 3\n")?;

        let result = compare_files(&file1_path, &file2_path)?;
        assert!(result.is_some()); // Differences expected

        let diff = result.unwrap();
        assert_eq!(diff.only_in_first, vec!["Line 1"]);
        assert_eq!(diff.only_in_second, vec!["Line 4"]);

        Ok(())
    }

    #[test]
    fn test_compare_files_nonexistent() -> Result<()> {
        let dir = Builder::new().prefix("tbcompare_test").tempdir()?;
        let file1_path = dir.path().join("file1.txt");
        let file2_path = dir.path().join("file2.txt");

        // Create only one file
        fs::write(&file1_path, "Header line\nLine 1\n")?;

        // Try to compare with non-existent file
        let result = compare_files(&file1_path, &file2_path);
        assert!(result.is_err()); // Should return an error

        Ok(())
    }

    #[test]
    fn test_generate_file_pairs() -> Result<()> {
        let dir1 = Builder::new().prefix("tbcompare_test1").tempdir()?;
        let dir2 = Builder::new().prefix("tbcompare_test2").tempdir()?;
        
        // Use file names that follow our pattern: SC_aaaaaaaa_yyyymmdd_tttN_AXX_Z
        let file1_path = dir1.path().join("SC_13260000_20190820_019N_A05_Z.txt");
        let file2_path = dir1.path().join("SC_13260000_20190820_020N_A01_Z.txt");
        let file3_path = dir2.path().join("SC_13260000_20190820_019N_A05_Z.txt"); // Same aaaaaaaa, yyyymmdd, AXX but different tttN
        let file4_path = dir2.path().join("SC_13260001_20190820_020N_A01_Z.txt"); // Different aaaaaaaa
        
        // Create test files
        fs::write(&file1_path, "Content 1")?;
        fs::write(&file2_path, "Content 2")?;
        fs::write(&file3_path, "Content 3")?;
        fs::write(&file4_path, "Content 4")?;
        
        let pairs = generate_file_pairs(dir1.path(), dir2.path())?;
        
        // Print debug information
        println!("Found {} pairs", pairs.len());
        for (i, (p1, p2)) in pairs.iter().enumerate() {
            println!("Pair {}: {:?} <=> {:?}", i, p1, p2);
        }
        
        // Should find one pair (SC_13260000_20190820_019N_A05_Z.txt matches with SC_13260000_20190820_019N_A05_Z.txt)
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, file1_path);
        assert_eq!(pairs[0].1, file3_path);

        Ok(())
    }
}

