use clap::Parser;
use std::path::PathBuf;
use std::fs::File;
use std::io::Write;
use tbcompare::{compare_files, generate_file_pairs};
use log::{info, error};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use anyhow::{Context, Result};
use chrono::Local;

/// Tool for comparing text files with specific naming conventions
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// First directory path
    #[clap(value_name = "DIR1")]
    dir1: PathBuf,

    /// Second directory path
    #[clap(value_name = "DIR2")]
    dir2: PathBuf,
    
    /// Number of parallel threads to use
    #[clap(short, long, default_value_t = 4)]
    threads: usize,
    
    /// Output report file path (optional)
    #[clap(short, long)]
    output: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    let args = Args::parse();
    
    // Set number of threads for rayon
    rayon::ThreadPoolBuilder::new().num_threads(args.threads).build_global()?;
    
    info!("开始比较目录 {:?} 和 {:?}，使用 {} 个线程", 
          args.dir1, args.dir2, args.threads);
    
    let file_pairs = generate_file_pairs(&args.dir1, &args.dir2)
        .context("生成文件对失败")?;
    
    if file_pairs.is_empty() {
        println!("在目录间未找到匹配的文件对。");
        return Ok(());
    }
    
    let file_pairs_count = file_pairs.len();
    println!("找到 {} 个文件对进行比较。", file_pairs_count);
    
    // Create a progress bar
    let pb = ProgressBar::new(file_pairs_count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars(">=-"),
    );
    
    // Process file pairs in parallel
    let results: Vec<_> = file_pairs
        .into_par_iter()
        .map(|(file1_path, file2_path)| {
            let result = compare_files(&file1_path, &file2_path);
            pb.inc(1);
            (file1_path, file2_path, result)
        })
        .collect();
    
    pb.finish_with_message("比较完成");
    
    // Generate report
    let mut report_content = String::new();
    let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
    
    report_content.push_str(&format!("文件比较报告 - 生成时间: {}\n", timestamp));
    report_content.push_str(&format!("比较目录: {:?} 和 {:?}\n", args.dir1, args.dir2));
    report_content.push_str(&format!("文件对数量: {}\n\n", file_pairs_count));
    
    let mut diff_count = 0;
    let mut error_count = 0;
    
    // Process results
    for (file1_path, file2_path, result) in results {
        // 从第一个路径中提取父目录名和文件名
        let file1_name = file1_path.file_name().unwrap_or_else(|| std::ffi::OsStr::new(""));
        let parent1_name = file1_path.parent()
            .and_then(|p| p.file_name())
            .unwrap_or_else(|| std::ffi::OsStr::new(""));
        let short_path1 = std::path::Path::new(parent1_name).join(file1_name);

        // 对第二个路径执行同样的操作
        let file2_name = file2_path.file_name().unwrap_or_else(|| std::ffi::OsStr::new(""));
        let parent2_name = file2_path.parent()
            .and_then(|p| p.file_name())
            .unwrap_or_else(|| std::ffi::OsStr::new(""));
        let short_path2 = std::path::Path::new(parent2_name).join(file2_name);
        match result {
            Ok(Some(diff)) => {
                diff_count += 1;
                report_content.push_str(&format!("发现差异: {} 和 {}\n", 
                                                short_path1.display(), short_path2.display()));
                if !diff.only_in_first.is_empty() {
                    report_content.push_str(&format!("仅在 {} 中的行:\n", short_path1.display()));
                    for line in &diff.only_in_first {
                        report_content.push_str(&format!("  {}\n", line));
                    }
                }
                if !diff.only_in_second.is_empty() {
                    report_content.push_str(&format!("仅在 {} 中的行:\n", short_path2.display()));
                    for line in &diff.only_in_second {
                        report_content.push_str(&format!("  {}\n", line));
                    }
                }
                report_content.push_str("\n");
            }
            Ok(None) => {
                // No differences - don't add to report to keep it concise
            }
            Err(e) => {
                error_count += 1;
                error!("比较 {} 和 {} 时出错: {}", 
                       file1_path.display(), file2_path.display(), e);
                report_content.push_str(&format!("比较错误: {} 和 {}\n错误信息: {}\n\n", 
                                                short_path1.display(), short_path2.display(), e));
            }
        }
    }
    
    report_content.push_str(&format!("总结:\n"));
    report_content.push_str(&format!("  - 发现差异的文件对: {}\n", diff_count));
    report_content.push_str(&format!("  - 比较出错的文件对: {}\n", error_count));
    report_content.push_str(&format!("  - 完全相同的文件对: {}\n", file_pairs_count - diff_count - error_count));
    
    // Output to console
    println!("\n比较完成！");
    println!("发现差异的文件对: {}", diff_count);
    println!("比较出错的文件对: {}", error_count);
    println!("完全相同的文件对: {}", file_pairs_count - diff_count - error_count);
    
    // Save report to file if requested
    if let Some(output_path) = &args.output {
        let report_path = if output_path.extension().is_none() {
            // Add timestamp to filename if no extension is provided
            let stem = output_path.file_stem().unwrap_or_default().to_string_lossy();
            let parent = output_path.parent().unwrap_or_else(|| std::path::Path::new("."));
            parent.join(format!("{}_{}.txt", stem, timestamp))
        } else {
            output_path.clone()
        };
        
        let mut file = File::create(&report_path)
            .with_context(|| format!("无法创建报告文件: {:?}", report_path))?;
        file.write_all(report_content.as_bytes())
            .with_context(|| format!("无法写入报告文件: {:?}", report_path))?;
        
        println!("详细报告已保存到: {:?}", report_path);
    } else {
        // Default report name with timestamp
        let report_filename = format!("comparison_report_{}.txt", timestamp);
        let mut file = File::create(&report_filename)
            .with_context(|| format!("无法创建报告文件: {}", report_filename))?;
        file.write_all(report_content.as_bytes())
            .with_context(|| format!("无法写入报告文件: {}", report_filename))?;
        
        println!("详细报告已保存到: {}", report_filename);
    }
    
    info!("文件比较完成");
    Ok(())
}
