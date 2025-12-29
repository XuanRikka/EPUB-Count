use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::process::exit;
use clap::Parser;
use zip::ZipArchive;
use scraper::Html;
use walkdir::{DirEntry, WalkDir};
use memmap2::Mmap;

/// 一个用于统计 EPUB 文件字数的小工具
///
/// 支持直接指定文件，或通过 `-w` 递归遍历目录。
#[derive(Parser)]
#[command(
    version,
    about,
    long_about = None,
)]
struct Cli
{
    /// 要统计的 EPUB 文件路径（支持多个）
    ///
    /// 可传入 `.epub` 文件，或配合 `-w` 传入目录。
    #[arg(required = true)]
    files: Vec<String>,

    /// 递归遍历目录（walk directories）
    ///
    /// 当传入的是目录时，自动查找其中所有 `.epub` 文件并统计。
    #[arg(short ,long, default_value_t = false, action = clap::ArgAction::SetTrue)]
    walk: bool
}


trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}


struct FileData
{
    filename: String,
    file: Box<dyn ReadSeek>
}


pub fn get_all_epub_walkdir<P: AsRef<Path>>(path: P) -> Vec<PathBuf> {
    fn is_epub(entry: &DirEntry) -> bool {
        entry.file_type().is_file()
            && entry
            .path()
            .extension()
            .map_or(false, |ext| ext.eq_ignore_ascii_case("epub"))
    }

    WalkDir::new(path)
        .into_iter()
        .filter_entry(|e| {
            e.file_type().is_dir() || is_epub(e)
        })
        .filter_map(|e| e.ok())
        .filter(is_epub)
        .map(|e| e.into_path())
        .collect()
}


fn html_word_count(string: &String) -> u64
{
    Html::parse_document(string)
        .root_element()
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
        .chars()
        .count() as u64
}


fn zip_xhtml_read<W: Read + Seek>(file: W) -> Vec<String> {
    let mut zip = ZipArchive::new(file).expect("读取zip文件时出现错误");

    let n = zip.len();
    let mut results = Vec::new();

    for i in 0..n {
        let mut file = zip.by_index(i).expect("遍历zip文件列表时出现错误");
        let name = file.name();

        if !name.ends_with(".xhtml") {
            continue;
        }
        if !(name.starts_with("OEBPS/Text/") || name.starts_with("EPUB/Text/")) {
            continue;
        }

        let size = file.size();
        let mut content = String::with_capacity(size as usize);

        file.read_to_string(&mut content).expect("读取xhtml文件时出现错误");
        results.push(content);
    }

    results
}

fn get_epub_word_count<W: Read + Seek>(file: W) -> u64
{
    let chars = zip_xhtml_read(file);
    let word_count: u64 = chars.iter().map(
        |s| html_word_count(s)
    ).sum::<u64>();

    word_count
}


fn main()
{
    let args = Cli::parse();

    let mut epub_renders: Vec<FileData> = Vec::new();

    for file in &args.files {
        let path = PathBuf::from(file.as_str());

        if !path.exists() {
            eprintln!("文件/目录 {} 不存在", file);
            continue;
        }

        if args.walk && path.is_dir()
        {
            for p in get_all_epub_walkdir(path.clone()) {
                let file = OpenOptions::new().
                    read(true).
                    write(false).
                    create(false).
                    open(p.clone()).
                    expect("打开文件时出现错误");
                let file_mmap = unsafe { Mmap::map(&file) };
                match file_mmap {
                    Ok(mmap) => {
                        let f = FileData{
                            filename : p.file_name().unwrap().to_string_lossy().to_string(),
                            file: Box::new(Cursor::new(mmap))
                        };
                        epub_renders.push(f)
                    },
                    Err(e) => {
                        eprintln!("警告：无法 mmap {}: {}", p.display(), e);
                        let f = FileData {
                            filename: p.file_name().unwrap().to_string_lossy().to_string(),
                            file: Box::new(file)
                        };
                        epub_renders.push(f);
                    }
                }
            }
        }
        else if !args.walk && path.is_dir()
        {
            continue;
        }
        else
        {
            let file = OpenOptions::new().
                read(true).
                write(false).
                create(false).
                open(path.clone()).
                expect("打开文件时出现错误");
            let file_mmap = unsafe { Mmap::map(&file) };
            match file_mmap {
                Ok(mmap) => {
                    let f = FileData{
                        filename : path.file_name().unwrap().to_string_lossy().to_string(),
                        file: Box::new(Cursor::new(mmap))
                    };
                    epub_renders.push(f)
                },
                Err(e) => {
                    eprintln!("警告：无法 mmap {}: {}", path.display(), e);
                    let f = FileData {
                        filename: path.file_name().unwrap().to_string_lossy().to_string(),
                        file: Box::new(file)
                    };
                    epub_renders.push(f);
                }
            }
        }
    }

    if epub_renders.is_empty()
    {
        eprintln!("没有找到任何EPUB文件");
        exit(0)
    }

    let mut total_word_count: u64 = 0;
    for f in epub_renders {
        let word_count = get_epub_word_count(f.file);
        println!("{}：{} 字", f.filename, word_count);
        total_word_count += word_count;
    }
    println!("总字数：{} 字", total_word_count)
}
