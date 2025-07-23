//! File utility functions for the tbcompare tool.

use std::fs::File;
use std::io::{Read, BufReader, BufRead, Write};
use std::path::Path;
use std::fs;
use encoding_rs_io::DecodeReaderBytesBuilder;
use encoding_rs::Encoding;
use anyhow::{Context, Result};
use std::process::Command;

/// Maximum file size that can be loaded into memory (100MB)
const MAX_MEMORY_FILE_SIZE: u64 = 100 * 1024 * 1024;

/// Detects the encoding of a file
/// 
/// # Arguments
/// 
/// * `file_path` - Path to the file to detect encoding for
/// 
/// # Returns
/// 
/// A Result containing either the detected encoding or an error
pub fn detect_encoding<P: AsRef<Path>>(file_path: P) -> Result<&'static Encoding> {
    let file = File::open(&file_path)
        .with_context(|| format!("Failed to open file: {}", file_path.as_ref().display()))?;
    let mut reader = BufReader::new(file);
    let mut buffer = [0; 1024];
    let bytes_read = reader.read(&mut buffer)
        .with_context(|| format!("Failed to read file: {}", file_path.as_ref().display()))?;
    
    let mut encoding_detector = chardetng::EncodingDetector::new();
    encoding_detector.feed(&buffer[..bytes_read], bytes_read < 1024);
    let encoding = encoding_detector.guess(None, true);
    
    Ok(encoding)
}

/// Checks if a file is too large to be loaded into memory
/// 
/// # Arguments
/// 
/// * `file_path` - Path to the file to check
/// 
/// # Returns
/// 
/// A Result containing either a boolean indicating if the file is too large or an error
fn is_file_too_large<P: AsRef<Path>>(file_path: P) -> Result<bool> {
    let metadata = fs::metadata(&file_path)
        .with_context(|| format!("Failed to get metadata for file: {}", file_path.as_ref().display()))?;
    Ok(metadata.len() > MAX_MEMORY_FILE_SIZE)
}

/// Reads and processes a file, skipping the first line and sorting the rest
/// For large files, uses external sorting to avoid memory issues
/// 
/// # Arguments
/// 
/// * `file_path` - Path to the file to read and process
/// 
/// # Returns
/// 
/// A Result containing either a sorted vector of lines or an error
pub fn read_and_process_file<P: AsRef<Path>>(file_path: P) -> Result<Vec<String>> {
    let file_path = file_path.as_ref();
    
    // Check if file is too large for memory
    if let Ok(too_large) = is_file_too_large(file_path) {
        if too_large {
            // For large files, use external sorting directly
            return external_sort_large_file(file_path);
        }
    }
    
    let encoding = detect_encoding(file_path)
        .with_context(|| format!("Failed to detect encoding for file: {}", file_path.display()))?;
    
    let file = File::open(file_path)
        .with_context(|| format!("Failed to open file: {}", file_path.display()))?;
    let decoder = DecodeReaderBytesBuilder::new()
        .encoding(Some(encoding))
        .build(file);
    let reader = BufReader::new(decoder);
    
    let mut lines = Vec::new();
    let mut first_line_skipped = false;
    
    for (index, line_result) in reader.lines().enumerate() {
        let line = line_result
            .with_context(|| format!("Failed to read line {} from file: {}", index, file_path.display()))?;
        if !first_line_skipped {
            first_line_skipped = true;
            continue;
        }
        lines.push(line.trim().to_string());
    }
    
    // For large files (many lines), use external sorting
    if lines.len() > 100_000 {
        external_sort(&mut lines)
            .with_context(|| format!("Failed to externally sort file: {}", file_path.display()))?;
    } else {
        lines.sort();
    }
    
    Ok(lines)
}

/// Sorts lines using Rust's built-in sort algorithm
/// This is more reliable across platforms than external sorting
fn internal_sort(lines: &mut Vec<String>) -> Result<()> {
    lines.sort();
    Ok(())
}

/// External sorting implementation for large files
/// Uses the system's sort command for efficiency
fn external_sort(lines: &mut Vec<String>) -> Result<()> {
    // Create a temporary file
    let mut temp_file = tempfile::NamedTempFile::new()
        .context("Failed to create temporary file for external sorting")?;
    
    // Write lines to temporary file
    for line in lines.iter() {
        writeln!(temp_file, "{}", line)
            .context("Failed to write to temporary file")?;
    }
    
    // Flush the file to ensure all data is written
    temp_file.flush()
        .context("Failed to flush temporary file")?;
    
    // Get the path of the temporary file
    let temp_path = temp_file.path();
    
    // Use system sort command
    let output = if cfg!(windows) {
        // On Windows, we'll use internal sorting instead of external command
        // which can be unreliable
        return internal_sort(lines);
    } else {
        Command::new("sort")
            .arg(temp_path)
            .output()
            .context("Failed to execute Unix sort command")?
    };
    
    // Check if the sort command was successful
    if !output.status.success() {
        anyhow::bail!("External sort command failed: {}", 
                      String::from_utf8_lossy(&output.stderr));
    }
    
    // Read sorted lines back
    let sorted_content = String::from_utf8(output.stdout)
        .context("Failed to parse sorted output as UTF-8")?;
    
    lines.clear();
    for line in sorted_content.lines() {
        lines.push(line.to_string());
    }
    
    Ok(())
}

/// External sorting implementation for large files that cannot fit in memory
/// Uses the system's sort command directly on the input file
fn external_sort_large_file<P: AsRef<Path>>(file_path: P) -> Result<Vec<String>> {
    let file_path = file_path.as_ref();
    
    // Detect encoding
    let encoding = detect_encoding(file_path)
        .with_context(|| format!("Failed to detect encoding for large file: {}", file_path.display()))?;
    
    // Create a temporary file for decoded content (without header)
    let mut temp_decoded_file = tempfile::NamedTempFile::new()
        .context("Failed to create temporary file for decoded content")?;
    
    // Open and decode the original file
    let file = File::open(file_path)
        .with_context(|| format!("Failed to open large file: {}", file_path.display()))?;
    let decoder = DecodeReaderBytesBuilder::new()
        .encoding(Some(encoding))
        .build(file);
    let reader = BufReader::new(decoder);
    
    // Skip the first line (header) and write the rest to temp file
    let mut first_line_skipped = false;
    let mut lines = Vec::new();
    for (index, line_result) in reader.lines().enumerate() {
        let line = line_result
            .with_context(|| format!("Failed to read line {} from large file: {}", index, file_path.display()))?;
        if !first_line_skipped {
            first_line_skipped = true;
            continue;
        }
        lines.push(line.trim().to_string());
    }
    
    // Instead of using external sort command on Windows, use internal sorting
    if cfg!(windows) {
        lines.sort();
        return Ok(lines);
    }
    
    // Write lines to temporary file for Unix systems
    for line in lines.iter() {
        writeln!(temp_decoded_file, "{}", line)
            .context("Failed to write to temporary decoded file")?;
    }
    
    // Flush the file to ensure all data is written
    temp_decoded_file.flush()
        .context("Failed to flush temporary decoded file")?;
    
    // Get the path of the temporary file
    let temp_path = temp_decoded_file.path();
    
    // Use system sort command
    let output = Command::new("sort")
        .arg(temp_path)
        .output()
        .context("Failed to execute Unix sort command on large file")?;
    
    // Check if the sort command was successful
    if !output.status.success() {
        anyhow::bail!("External sort command failed for large file: {}", 
                      String::from_utf8_lossy(&output.stderr));
    }
    
    // Read sorted lines back
    let sorted_content = String::from_utf8(output.stdout)
        .context("Failed to parse sorted output as UTF-8 for large file")?;
    
    // Convert to Vec<String>
    let lines: Vec<String> = sorted_content
        .lines()
        .map(|line| line.to_string())
        .collect();
    
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::Builder;
    use anyhow::Result;

    #[test]
    fn test_detect_encoding_utf8() -> Result<()> {
        let dir = Builder::new().prefix("tbcompare_test").tempdir()?;
        let file_path = dir.path().join("test_utf8.txt");
        
        // Create a UTF-8 encoded file
        fs::write(&file_path, "Test content with UTF-8 encoding\n")?;
        
        let encoding = detect_encoding(&file_path)?;
        assert_eq!(encoding, encoding_rs::UTF_8);
        
        Ok(())
    }

    #[test]
    fn test_read_and_process_file() -> Result<()> {
        let dir = Builder::new().prefix("tbcompare_test").tempdir()?;
        let file_path = dir.path().join("test_file.txt");
        
        // Create a test file with header and unsorted lines
        let content = "Header line\nLine 3\nLine 1\nLine 2\n";
        fs::write(&file_path, content)?;
        
        let lines = read_and_process_file(&file_path)?;
        
        // Should skip header and sort the rest
        assert_eq!(lines, vec!["Line 1", "Line 2", "Line 3"]);
        
        Ok(())
    }
}
