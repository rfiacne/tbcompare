# tbcompare - Rust版本

tbcompare 是一个使用 Rust 编写的命令行小工具，专门用于分析和比较多次生成的投保基金导出文件。

## 功能特性

- 比较遵循特定命名模式的文件对
- 自动检测文件编码
- 跳过每个文件的第一行并对剩余行进行排序
- 报告文件之间的差异
- 通过命令行参数进行配置
- 进度条显示比较进度
- 并行处理以提高性能
- 大文件的外部排序以避免内存问题
- 使用系统命令（Windows上的fc.exe或Unix上的diff）进行快速文件比较
- 生成带时间戳的比较报告（默认保存为comparison_report_YYYYMMDD_HHMMSS.txt）
- 支持自定义报告输出路径

## 构建

要构建项目，您需要安装Rust和Cargo。然后运行：

```bash
cargo build
```

## 使用方法

```bash
cargo run --bin tbcompare [dir1] [dir2]
```

```bash
cargo run --bin tbcompare [options] [dir1] [dir2]
```

### 参数

- `dir1`: 包含要比较的文件的第一个目录路径
- `dir2`: 包含要比较的文件的第二个目录路径
- `-t, --threads <threads>`: 要使用的并行线程数（默认：4）
- `-o, --output <output>`: 指定报告输出文件路径（可选）

### 示例

基本使用：
```bash
cargo run --bin tbcompare test/sample1 test/sample2
```

使用自定义线程数：
```bash
cargo run --bin tbcompare -t 8 test/sample1 test/sample2
```

指定报告输出路径：
```bash
cargo run --bin tbcompare -o report.txt test/sample1 test/sample2
```

使用自定义报告名（会自动添加时间戳）：
```bash
cargo run --bin tbcompare -o my_comparison test/sample1 test/sample2
```

## 依赖库

- `clap`: 命令行参数解析
- `encoding_rs`: 字符编码检测和转换
- `encoding_rs_io`: encoding_rs的IO包装器
- `chardetng`: 字符编码检测
- `log`: 日志门面
- `env_logger`: 日志实现
- `indicatif`: 进度条
- `rayon`: 并行处理
- `anyhow`: 改进的错误处理
- `tempfile`: 临时文件处理用于外部排序
- `chrono`: 时间戳生成

## 项目结构

```
src/
├── main.rs         # 入口点，包含CLI参数解析
├── lib.rs          # 库模块导出
├── file_utils.rs   # 文件处理工具
└── comparison.rs   # 文件比较逻辑
```

## 性能考虑

为了最大化比较效率，tbcompare 在 Windows 系统上会优先调用 fc.exe 命令，在 Unix-like 系统上则使用 diff 命令。这两个系统原生命令能极快地判断文件是否一致。如果系统命令不可用或无法确认文件一致性，工具将自动切换到内置的、经过优化的比较算法来完成任务。